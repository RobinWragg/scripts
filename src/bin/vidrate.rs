use anyhow::{Context, Result};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

const INITIAL_RATING: f64 = 1500.0;
const K_FACTOR: f64 = 32.0;

#[derive(Parser)]
#[command(name = "vidrate")]
#[command(about = "ELO rating system for video files")]
struct Args {
    /// Path to folder containing video files
    video_folder: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VideoRating {
    filename: String,
    rating: f64,
    wins: u32,
    losses: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RatingsData {
    videos: HashMap<String, VideoRating>,
}

#[derive(Clone)]
struct AppState {
    video_folder: PathBuf,
    ratings: Arc<Mutex<RatingsData>>,
}

#[derive(Serialize)]
struct VideoPair {
    video1: String,
    video2: String,
}

#[derive(Deserialize)]
struct ComparisonResult {
    winner: String,
    loser: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if !args.video_folder.exists() {
        anyhow::bail!("Video folder does not exist: {:?}", args.video_folder);
    }

    let ratings_path = args.video_folder.join("ratings.json");
    let mut ratings = load_or_create_ratings(&ratings_path)?;

    // Get current video files
    let video_files = scan_video_files(&args.video_folder)?;

    // Clean up ratings for deleted videos
    ratings
        .videos
        .retain(|filename, _| video_files.contains(filename));

    // Add new videos with initial ratings
    for filename in &video_files {
        ratings
            .videos
            .entry(filename.clone())
            .or_insert_with(|| VideoRating {
                filename: filename.clone(),
                rating: INITIAL_RATING,
                wins: 0,
                losses: 0,
            });
    }

    // Save updated ratings
    save_ratings(&ratings_path, &ratings)?;

    let state = AppState {
        video_folder: args.video_folder.clone(),
        ratings: Arc::new(Mutex::new(ratings)),
    };

    println!("Starting video rating server at http://localhost:3000");
    println!("Video folder: {:?}", args.video_folder);
    println!("Found {} video files", video_files.len());

    // Open browser
    if let Err(e) = open::that("http://localhost:3000") {
        eprintln!("Failed to open browser: {}", e);
    }

    tokio::runtime::Runtime::new()?.block_on(async { run_server(state).await })
}

fn scan_video_files(folder: &PathBuf) -> Result<Vec<String>> {
    let mut video_files = Vec::new();

    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext.eq_ignore_ascii_case("mp4") {
                    if let Some(filename) = path.file_name() {
                        video_files.push(filename.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    Ok(video_files)
}

fn load_or_create_ratings(path: &PathBuf) -> Result<RatingsData> {
    if path.exists() {
        let content = fs::read_to_string(path)?;
        let ratings: RatingsData = serde_json::from_str(&content)?;
        Ok(ratings)
    } else {
        Ok(RatingsData {
            videos: HashMap::new(),
        })
    }
}

fn save_ratings(path: &PathBuf, ratings: &RatingsData) -> Result<()> {
    let json = serde_json::to_string_pretty(ratings)?;
    fs::write(path, json)?;
    Ok(())
}

fn calculate_elo(winner_rating: f64, loser_rating: f64) -> (f64, f64) {
    let expected_winner = 1.0 / (1.0 + 10_f64.powf((loser_rating - winner_rating) / 400.0));
    let expected_loser = 1.0 / (1.0 + 10_f64.powf((winner_rating - loser_rating) / 400.0));

    let new_winner_rating = winner_rating + K_FACTOR * (1.0 - expected_winner);
    let new_loser_rating = loser_rating + K_FACTOR * (0.0 - expected_loser);

    (new_winner_rating, new_loser_rating)
}

async fn run_server(state: AppState) -> Result<()> {
    let app = Router::new()
        .route("/", get(serve_compare_page))
        .route("/rankings", get(serve_rankings_page))
        .route("/api/pair", get(get_random_pair))
        .route("/api/compare", post(submit_comparison))
        .route("/api/rankings", get(get_rankings))
        .route("/api/open/:filename", post(open_video))
        .route("/videos/:filename", get(serve_video))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .context("Failed to bind to port 3000")?;

    axum::serve(listener, app).await.context("Server error")?;

    Ok(())
}

async fn serve_compare_page() -> Html<String> {
    let html = r###"
<!DOCTYPE html>
<html>
<head>
    <title>Video Rating</title>
    <style>
        body {
            font-family: Arial, sans-serif;
            margin: 0;
            padding: 20px;
            background: #1a1a1a;
            color: #fff;
        }
        .container {
            max-width: 1400px;
            margin: 0 auto;
        }
        h1 {
            text-align: center;
        }
        .video-container {
            display: flex;
            gap: 20px;
            margin: 20px 0;
            justify-content: center;
        }
        .video-wrapper {
            flex: 1;
            max-width: 640px;
            text-align: center;
        }
        video {
            width: 100%;
            max-height: 480px;
            background: #000;
        }
        button {
            padding: 15px 30px;
            font-size: 18px;
            margin: 10px;
            cursor: pointer;
            background: #4CAF50;
            color: white;
            border: none;
            border-radius: 5px;
        }
        button:hover {
            background: #45a049;
        }
        .nav {
            text-align: center;
            margin-bottom: 20px;
        }
        .nav a {
            color: #4CAF50;
            text-decoration: none;
            font-size: 18px;
        }
        .nav a:hover {
            text-decoration: underline;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="nav">
            <a href="/rankings">View Rankings</a>
        </div>
        <h1>Video Rating</h1>
        <div class="video-container">
            <div class="video-wrapper">
                <video id="video1" controls autoplay loop muted></video>
                <br>
                <button onclick="selectWinner('video1')">This One is Better</button>
            </div>
            <div class="video-wrapper">
                <video id="video2" controls autoplay loop muted></video>
                <br>
                <button onclick="selectWinner('video2')">This One is Better</button>
            </div>
        </div>
    </div>

    <script>
        let currentPair = null;

        async function loadPair() {
            try {
                const response = await fetch('/api/pair');
                const pair = await response.json();
                currentPair = pair;

                const video1 = document.getElementById('video1');
                const video2 = document.getElementById('video2');

                video1.src = '/videos/' + encodeURIComponent(pair.video1);
                video2.src = '/videos/' + encodeURIComponent(pair.video2);

                video1.load();
                video2.load();
                video1.play();
                video2.play();
            } catch (error) {
                console.error('Error loading pair:', error);
                alert('Error loading videos. Check console for details.');
            }
        }

        async function selectWinner(videoId) {
            if (!currentPair) return;

            const winner = videoId === 'video1' ? currentPair.video1 : currentPair.video2;
            const loser = videoId === 'video1' ? currentPair.video2 : currentPair.video1;

            try {
                await fetch('/api/compare', {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                    },
                    body: JSON.stringify({ winner, loser }),
                });

                loadPair();
            } catch (error) {
                console.error('Error submitting comparison:', error);
                alert('Error submitting comparison. Check console for details.');
            }
        }

        loadPair();
    </script>
</body>
</html>
    "###;
    Html(html.to_string())
}

async fn serve_rankings_page() -> Html<String> {
    let html = r###"
<!DOCTYPE html>
<html>
<head>
    <title>Video Rankings</title>
    <style>
        body {
            font-family: Arial, sans-serif;
            margin: 0;
            padding: 20px;
            background: #1a1a1a;
            color: #fff;
        }
        .container {
            max-width: 1000px;
            margin: 0 auto;
        }
        h1 {
            text-align: center;
        }
        .nav {
            text-align: center;
            margin-bottom: 20px;
        }
        .nav a {
            color: #4CAF50;
            text-decoration: none;
            font-size: 18px;
        }
        .nav a:hover {
            text-decoration: underline;
        }
        table {
            width: 100%;
            border-collapse: collapse;
            margin-top: 20px;
        }
        th, td {
            padding: 12px;
            text-align: left;
            border-bottom: 1px solid #444;
        }
        th {
            background: #333;
            font-weight: bold;
        }
        tr:hover {
            background: #2a2a2a;
        }
        .filename {
            cursor: pointer;
            color: #4CAF50;
        }
        .filename:hover {
            text-decoration: underline;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="nav">
            <a href="/">Back to Comparison</a>
        </div>
        <h1>Video Rankings</h1>
        <table>
            <thead>
                <tr>
                    <th>Rank</th>
                    <th>Filename</th>
                    <th>Rating</th>
                    <th>Wins</th>
                    <th>Losses</th>
                    <th>Win Rate</th>
                </tr>
            </thead>
            <tbody id="rankings-body">
            </tbody>
        </table>
    </div>

    <script>
        async function loadRankings() {
            try {
                const response = await fetch('/api/rankings');
                const rankings = await response.json();

                const tbody = document.getElementById('rankings-body');
                tbody.innerHTML = '';

                rankings.forEach((video, index) => {
                    const row = document.createElement('tr');
                    const totalGames = video.wins + video.losses;
                    const winRate = totalGames > 0 ? ((video.wins / totalGames) * 100).toFixed(1) : '0.0';

                    row.innerHTML = `
                        <td>${index + 1}</td>
                        <td class="filename" onclick="openVideo('${video.filename}')">${video.filename}</td>
                        <td>${Math.round(video.rating)}</td>
                        <td>${video.wins}</td>
                        <td>${video.losses}</td>
                        <td>${winRate}%</td>
                    `;
                    tbody.appendChild(row);
                });
            } catch (error) {
                console.error('Error loading rankings:', error);
            }
        }

        async function openVideo(filename) {
            try {
                await fetch('/api/open/' + encodeURIComponent(filename), {
                    method: 'POST',
                });
            } catch (error) {
                console.error('Error opening video:', error);
            }
        }

        loadRankings();
        setInterval(loadRankings, 2000);
    </script>
</body>
</html>
    "###;
    Html(html.to_string())
}

async fn get_random_pair(State(state): State<AppState>) -> Result<Json<VideoPair>, StatusCode> {
    let ratings = state.ratings.lock().unwrap();

    if ratings.videos.len() < 2 {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Separate videos into unrated (no comparisons) and rated
    let mut unrated: Vec<String> = Vec::new();
    let mut rated: Vec<String> = Vec::new();

    for (filename, video) in &ratings.videos {
        if video.wins == 0 && video.losses == 0 {
            unrated.push(filename.clone());
        } else {
            rated.push(filename.clone());
        }
    }

    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();

    let video1: String;
    let video2: String;

    // Prioritize unrated videos
    if unrated.len() >= 2 {
        // Pick two unrated videos
        let sample: Vec<_> = unrated.choose_multiple(&mut rng, 2).cloned().collect();
        video1 = sample[0].clone();
        video2 = sample[1].clone();
    } else if unrated.len() == 1 {
        // Pick one unrated and one rated
        video1 = unrated[0].clone();
        video2 = rated.choose(&mut rng).unwrap().clone();
    } else {
        // All videos have been rated, pick two randomly
        let all_videos: Vec<String> = rated;
        let sample: Vec<_> = all_videos.choose_multiple(&mut rng, 2).cloned().collect();
        video1 = sample[0].clone();
        video2 = sample[1].clone();
    }

    Ok(Json(VideoPair { video1, video2 }))
}

async fn submit_comparison(
    State(state): State<AppState>,
    Json(result): Json<ComparisonResult>,
) -> StatusCode {
    let mut ratings = state.ratings.lock().unwrap();

    let winner_rating = ratings
        .videos
        .get(&result.winner)
        .map(|v| v.rating)
        .unwrap_or(INITIAL_RATING);
    let loser_rating = ratings
        .videos
        .get(&result.loser)
        .map(|v| v.rating)
        .unwrap_or(INITIAL_RATING);

    let (new_winner_rating, new_loser_rating) = calculate_elo(winner_rating, loser_rating);

    if let Some(winner) = ratings.videos.get_mut(&result.winner) {
        winner.rating = new_winner_rating;
        winner.wins += 1;
    }

    if let Some(loser) = ratings.videos.get_mut(&result.loser) {
        loser.rating = new_loser_rating;
        loser.losses += 1;
    }

    let ratings_path = state.video_folder.join("ratings.json");
    if let Err(e) = save_ratings(&ratings_path, &ratings) {
        eprintln!("Error saving ratings: {}", e);
    }

    StatusCode::OK
}

async fn get_rankings(State(state): State<AppState>) -> Json<Vec<VideoRating>> {
    let ratings = state.ratings.lock().unwrap();
    let mut videos: Vec<VideoRating> = ratings.videos.values().cloned().collect();
    videos.sort_by(|a, b| b.rating.partial_cmp(&a.rating).unwrap());
    Json(videos)
}

async fn open_video(State(state): State<AppState>, Path(filename): Path<String>) -> StatusCode {
    let video_path = state.video_folder.join(&filename);

    if !video_path.exists() {
        return StatusCode::NOT_FOUND;
    }

    match Command::new("open").arg(&video_path).spawn() {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn serve_video(
    State(state): State<AppState>,
    Path(filename): Path<String>,
) -> Result<Response, StatusCode> {
    let video_path = state.video_folder.join(&filename);

    if !video_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    match fs::read(&video_path) {
        Ok(content) => {
            Ok((StatusCode::OK, [("Content-Type", "video/mp4")], content).into_response())
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
