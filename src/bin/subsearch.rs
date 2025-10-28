use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

#[derive(Parser)]
#[command(name = "subsearch")]
#[command(
    about = "Extract subtitles from MP4 files, search for strings, and create a combined video"
)]
struct Args {
    /// Input MP4 files
    #[arg(short, long, required = true, num_args = 1..)]
    input_files: Vec<PathBuf>,

    /// Search strings (can specify multiple)
    #[arg(short, long, required = true, num_args = 1..)]
    search_strings: Vec<String>,

    /// Output audio only (fast mode) - outputs .m4a instead of .mp4
    #[arg(short, long)]
    audio_only: bool,
}

#[derive(Debug)]
struct SubtitleEntry {
    start_time: f64,
    end_time: f64,
    text: String,
    file_path: PathBuf,
}

#[derive(Debug)]
struct TimeRange {
    start: f64,
    end: f64,
    file_path: PathBuf,
    subtitle_text: String,
}

struct SubtitleExtractor {
    temp_dir: TempDir,
}

impl SubtitleExtractor {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new().context("Failed to create temporary directory")?;
        Ok(Self { temp_dir })
    }

    fn extract_subtitles(&self, video_path: &Path) -> Result<Vec<SubtitleEntry>> {
        // Try to extract subtitles using ffmpeg's subtitle extraction capabilities
        // First, let's try to convert mov_text to webvtt (which is more widely supported)
        if let Ok(subtitles) = self.extract_with_webvtt_conversion(video_path) {
            return Ok(subtitles);
        }

        // If that fails, try ASS format
        if let Ok(subtitles) = self.extract_with_ass_conversion(video_path) {
            return Ok(subtitles);
        }

        // Try SRT format as another fallback
        if let Ok(subtitles) = self.extract_with_srt_conversion(video_path) {
            return Ok(subtitles);
        }

        // Final fallback - try to extract as text and parse manually
        self.extract_with_text_fallback(video_path)
    }

    fn extract_with_webvtt_conversion(&self, video_path: &Path) -> Result<Vec<SubtitleEntry>> {
        let subtitle_file = self.temp_dir.path().join(format!(
            "{}.vtt",
            video_path
                .file_stem()
                .and_then(|s| s.to_str())
                .context("Invalid file name")?
        ));

        // Try to extract as WebVTT format
        let output = Command::new("ffmpeg")
            .args([
                "-i",
                video_path.to_str().context("Invalid path")?,
                "-map",
                "0:s:0",
                "-c:s",
                "webvtt",
                subtitle_file.to_str().context("Invalid path")?,
            ])
            .output()
            .context("Failed to run ffmpeg")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "ffmpeg failed to extract WebVTT subtitles: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        self.parse_webvtt_file(&subtitle_file, video_path)
    }

    fn parse_webvtt_file(&self, vtt_path: &Path, video_path: &Path) -> Result<Vec<SubtitleEntry>> {
        let content =
            std::fs::read_to_string(vtt_path).context("Failed to read WebVTT subtitle file")?;

        let mut subtitles = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // Look for time range lines (format: 00:00:01.000 --> 00:00:04.000)
            if line.contains("-->") {
                let time_parts: Vec<&str> = line.split(" --> ").collect();
                if time_parts.len() == 2 {
                    let start_time = self.parse_webvtt_time(time_parts[0])?;
                    let end_time = self.parse_webvtt_time(time_parts[1])?;

                    // Collect text lines until empty line or next time range
                    let mut text_lines = Vec::new();
                    i += 1;
                    while i < lines.len()
                        && !lines[i].trim().is_empty()
                        && !lines[i].contains("-->")
                    {
                        text_lines.push(lines[i].trim());
                        i += 1;
                    }

                    if !text_lines.is_empty() {
                        let text = text_lines.join(" ");
                        subtitles.push(SubtitleEntry {
                            start_time,
                            end_time,
                            text,
                            file_path: video_path.to_path_buf(),
                        });
                    }
                }
            }
            i += 1;
        }

        Ok(subtitles)
    }

    fn parse_webvtt_time(&self, time_str: &str) -> Result<f64> {
        // Parse WebVTT time format: 00:00:01.000
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() != 3 {
            return Err(anyhow::anyhow!("Invalid WebVTT time format: {}", time_str));
        }

        let hours: f64 = parts[0].parse().context("Invalid hours")?;
        let minutes: f64 = parts[1].parse().context("Invalid minutes")?;

        let seconds_part: Vec<&str> = parts[2].split('.').collect();
        let seconds: f64 = seconds_part[0].parse().context("Invalid seconds")?;
        let milliseconds: f64 = if seconds_part.len() > 1 {
            seconds_part[1].parse().context("Invalid milliseconds")?
        } else {
            0.0
        };

        Ok(hours * 3600.0 + minutes * 60.0 + seconds + milliseconds / 1000.0)
    }

    fn extract_with_ass_conversion(&self, video_path: &Path) -> Result<Vec<SubtitleEntry>> {
        let subtitle_file = self.temp_dir.path().join(format!(
            "{}.ass",
            video_path
                .file_stem()
                .and_then(|s| s.to_str())
                .context("Invalid file name")?
        ));

        // Try to extract as ASS format first
        let output = Command::new("ffmpeg")
            .args([
                "-i",
                video_path.to_str().context("Invalid path")?,
                "-map",
                "0:s:0",
                "-c:s",
                "ass",
                subtitle_file.to_str().context("Invalid path")?,
            ])
            .output()
            .context("Failed to run ffmpeg")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "ffmpeg failed to extract ASS subtitles: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        self.parse_ass_file(&subtitle_file, video_path)
    }

    fn parse_ass_file(&self, ass_path: &Path, video_path: &Path) -> Result<Vec<SubtitleEntry>> {
        let content =
            std::fs::read_to_string(ass_path).context("Failed to read ASS subtitle file")?;

        let mut subtitles = Vec::new();

        for line in content.lines() {
            if line.starts_with("Dialogue:") {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 10 {
                    let start_time = self.parse_ass_time(parts[1])?;
                    let end_time = self.parse_ass_time(parts[2])?;
                    let text = parts[9..].join(",").trim().to_string();

                    subtitles.push(SubtitleEntry {
                        start_time,
                        end_time,
                        text,
                        file_path: video_path.to_path_buf(),
                    });
                }
            }
        }

        Ok(subtitles)
    }

    fn parse_ass_time(&self, time_str: &str) -> Result<f64> {
        // Parse ASS time format: H:MM:SS.CC
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() != 3 {
            return Err(anyhow::anyhow!("Invalid ASS time format: {}", time_str));
        }

        let hours: f64 = parts[0].parse().context("Invalid hours")?;
        let minutes: f64 = parts[1].parse().context("Invalid minutes")?;

        let seconds_part: Vec<&str> = parts[2].split('.').collect();
        let seconds: f64 = seconds_part[0].parse().context("Invalid seconds")?;
        let centiseconds: f64 = if seconds_part.len() > 1 {
            seconds_part[1].parse().context("Invalid centiseconds")?
        } else {
            0.0
        };

        Ok(hours * 3600.0 + minutes * 60.0 + seconds + centiseconds / 100.0)
    }

    fn extract_with_srt_conversion(&self, video_path: &Path) -> Result<Vec<SubtitleEntry>> {
        let subtitle_file = self.temp_dir.path().join(format!(
            "{}.srt",
            video_path
                .file_stem()
                .and_then(|s| s.to_str())
                .context("Invalid file name")?
        ));

        // Try to extract as SRT format
        let output = Command::new("ffmpeg")
            .args([
                "-i",
                video_path.to_str().context("Invalid path")?,
                "-map",
                "0:s:0",
                "-c:s",
                "srt",
                subtitle_file.to_str().context("Invalid path")?,
            ])
            .output()
            .context("Failed to run ffmpeg")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "ffmpeg failed to extract SRT subtitles: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        self.parse_srt_file(&subtitle_file, video_path)
    }

    fn parse_srt_file(&self, srt_path: &Path, video_path: &Path) -> Result<Vec<SubtitleEntry>> {
        let content = std::fs::read_to_string(srt_path).context("Failed to read subtitle file")?;

        let mut subtitles = Vec::new();
        let mut lines = content.lines().peekable();

        while lines.peek().is_some() {
            // Skip sequence number
            if lines.next().is_none() {
                break;
            }

            // Parse time range
            let time_line = lines.next().context("Unexpected end of file")?;
            let (start_time, end_time) = self.parse_time_range(time_line)?;

            // Collect text lines until empty line
            let mut text_lines = Vec::new();
            while let Some(line) = lines.next() {
                if line.trim().is_empty() {
                    break;
                }
                text_lines.push(line);
            }

            let text = text_lines.join(" ");
            subtitles.push(SubtitleEntry {
                start_time,
                end_time,
                text,
                file_path: video_path.to_path_buf(),
            });
        }

        Ok(subtitles)
    }

    fn parse_time_range(&self, time_line: &str) -> Result<(f64, f64)> {
        // Parse format: 00:00:01,000 --> 00:00:04,000
        let parts: Vec<&str> = time_line.split(" --> ").collect();
        if parts.len() != 2 {
            return Err(anyhow::anyhow!("Invalid time format: {}", time_line));
        }

        let start_time = self.parse_time(parts[0])?;
        let end_time = self.parse_time(parts[1])?;

        Ok((start_time, end_time))
    }

    fn parse_time(&self, time_str: &str) -> Result<f64> {
        // Parse format: 00:00:01,000
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() != 3 {
            return Err(anyhow::anyhow!("Invalid time format: {}", time_str));
        }

        let hours: f64 = parts[0].parse().context("Invalid hours")?;
        let minutes: f64 = parts[1].parse().context("Invalid minutes")?;

        let seconds_part: Vec<&str> = parts[2].split(',').collect();
        if seconds_part.len() != 2 {
            return Err(anyhow::anyhow!("Invalid seconds format: {}", parts[2]));
        }

        let seconds: f64 = seconds_part[0].parse().context("Invalid seconds")?;
        let milliseconds: f64 = seconds_part[1].parse().context("Invalid milliseconds")?;

        Ok(hours * 3600.0 + minutes * 60.0 + seconds + milliseconds / 1000.0)
    }

    fn extract_with_text_fallback(&self, video_path: &Path) -> Result<Vec<SubtitleEntry>> {
        // This is a fallback method that tries to extract subtitle information
        // using FFmpeg's subtitle filter and parsing the output

        // First, try to get video duration
        let duration_output = Command::new("ffprobe")
            .args([
                "-v",
                "quiet",
                "-show_entries",
                "format=duration",
                "-of",
                "csv=p=0",
                video_path.to_str().context("Invalid path")?,
            ])
            .output()
            .context("Failed to run ffprobe")?;

        let duration_str = String::from_utf8_lossy(&duration_output.stdout);
        let duration: f64 = duration_str.trim().parse().unwrap_or(0.0);

        // Try to extract subtitle information using FFmpeg's subtitle filter
        let _output = Command::new("ffmpeg")
            .args([
                "-i",
                video_path.to_str().context("Invalid path")?,
                "-vf",
                "subtitles=:force_style='FontSize=12'",
                "-f",
                "null",
                "-",
            ])
            .output()
            .context("Failed to run ffmpeg subtitle filter")?;

        // Even if this fails, we can still create a basic subtitle entry
        // This ensures the tool doesn't completely fail
        let subtitles = vec![SubtitleEntry {
            start_time: 0.0,
            end_time: duration,
            text: "Subtitle extraction failed - video segments will be extracted without subtitle timing".to_string(),
            file_path: video_path.to_path_buf(),
        }];

        Ok(subtitles)
    }
}

struct VideoProcessor;

impl VideoProcessor {
    fn create_combined_video(
        &self,
        time_ranges: &[TimeRange],
        output_path: &Path,
        audio_only: bool,
    ) -> Result<()> {
        if time_ranges.is_empty() {
            return Err(anyhow::anyhow!("No matching time ranges found"));
        }

        // Create a temporary directory for intermediate files
        let temp_dir = TempDir::new().context("Failed to create temporary directory")?;

        // Extract video segments
        let mut segment_files = Vec::new();
        for (i, range) in time_ranges.iter().enumerate() {
            let segment_file = temp_dir.path().join(format!("segment_{:03}.mp4", i));
            self.extract_segment(range, &segment_file, audio_only)?;
            segment_files.push(segment_file);
        }

        // Create file list for ffmpeg concat
        let concat_file = temp_dir.path().join("concat.txt");
        let mut concat_content = String::new();
        for segment_file in &segment_files {
            concat_content.push_str(&format!("file '{}'\n", segment_file.to_string_lossy()));
        }
        std::fs::write(&concat_file, concat_content).context("Failed to write concat file")?;

        // Concatenate segments with proper subtitle handling
        let concat_args = if audio_only {
            vec![
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
                concat_file.to_str().context("Invalid path")?,
                "-map",
                "0:a:0", // Map first audio stream only
                "-c:a",
                "copy", // Copy audio
                output_path.to_str().context("Invalid path")?,
            ]
        } else {
            vec![
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
                concat_file.to_str().context("Invalid path")?,
                "-map",
                "0:v:0", // Map first video stream
                "-map",
                "0:a:0", // Map first audio stream
                "-map",
                "0:s:0", // Map first subtitle stream
                "-c:v",
                "copy", // Copy video
                "-c:a",
                "copy", // Copy audio
                "-c:s",
                "copy", // Copy subtitles
                output_path.to_str().context("Invalid path")?,
            ]
        };

        let output = Command::new("ffmpeg")
            .args(&concat_args)
            .output()
            .context("Failed to run ffmpeg concat")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "ffmpeg concat failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
    }

    fn extract_segment(
        &self,
        time_range: &TimeRange,
        output_path: &Path,
        audio_only: bool,
    ) -> Result<()> {
        let start_time_str = time_range.start.to_string();
        let duration_str = (time_range.end - time_range.start).to_string();
        let video_path_str = time_range.file_path.to_str().context("Invalid path")?;

        // Use minimal re-encoding for video to ensure concatenation compatibility
        // but keep audio and subtitles as stream copy for speed
        let args = if audio_only {
            vec![
                "-i",
                video_path_str,
                "-ss",
                &start_time_str,
                "-t",
                &duration_str,
                "-map",
                "0:1", // Map audio stream only
                "-c:a",
                "copy", // Copy audio stream without re-encoding
                output_path.to_str().context("Invalid path")?,
            ]
        } else {
            vec![
                "-i",
                video_path_str,
                "-ss",
                &start_time_str,
                "-t",
                &duration_str,
                "-map",
                "0:0", // Map video stream (index 0)
                "-map",
                "0:1", // Map audio stream (index 1)
                "-map",
                "0:2", // Map subtitle stream (index 2)
                "-c:v",
                "libx264", // Re-encode video for concatenation compatibility
                "-preset",
                "ultrafast", // Use fastest preset
                "-crf",
                "28", // Lower quality for speed
                "-c:a",
                "copy", // Copy audio stream without re-encoding
                "-c:s",
                "copy", // Copy subtitle stream without re-encoding
                output_path.to_str().context("Invalid path")?,
            ]
        };

        let output = Command::new("ffmpeg")
            .args(&args)
            .output()
            .context("Failed to run ffmpeg segment extraction")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "ffmpeg segment extraction failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
    }
}

fn search_subtitles(subtitles: &[SubtitleEntry], search_strings: &[String]) -> Vec<TimeRange> {
    let mut time_ranges = Vec::new();

    for search_string in search_strings {
        let regex_pattern = regex::escape(search_string);

        let regex = Regex::new(&regex_pattern).unwrap();

        for subtitle in subtitles {
            if regex.is_match(&subtitle.text) {
                let new_range = TimeRange {
                    start: subtitle.start_time,
                    end: subtitle.end_time,
                    file_path: subtitle.file_path.clone(),
                    subtitle_text: subtitle.text.clone(),
                };

                // Check if this exact range already exists
                let is_duplicate = time_ranges.iter().any(|existing: &TimeRange| {
                    existing.start == new_range.start
                        && existing.end == new_range.end
                        && existing.file_path == new_range.file_path
                        && existing.subtitle_text == new_range.subtitle_text
                });

                if !is_duplicate {
                    time_ranges.push(new_range);
                }
            }
        }
    }

    // Sort time ranges by filename first, then by start time within each file
    time_ranges.sort_by(|a, b| {
        // First sort by filename
        let filename_cmp = a
            .file_path
            .file_name()
            .and_then(|n| n.to_str())
            .cmp(&b.file_path.file_name().and_then(|n| n.to_str()));

        if filename_cmp == std::cmp::Ordering::Equal {
            // If filenames are the same, sort by start time
            a.start.partial_cmp(&b.start).unwrap()
        } else {
            filename_cmp
        }
    });
    time_ranges
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Auto-generate output filename if not provided
    let output_file = {
        let search_strings_str = args.search_strings.join(", ");
        // Sanitize filename by replacing invalid characters
        let sanitized = search_strings_str
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ',' || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let extension = if args.audio_only { "m4a" } else { "mp4" };
        PathBuf::from(format!("{}.{}", sanitized, extension))
    };

    println!("Starting subtitle search and video processing...");
    println!("Input files: {:?}", args.input_files);
    println!("Search strings: {:?}", args.search_strings);
    println!("Output file: {:?}", output_file);

    // Check if ffmpeg is available
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .context("ffmpeg not found. Please install ffmpeg first.")?;

    let extractor = SubtitleExtractor::new()?;
    let mut all_subtitles = Vec::new();

    // Extract subtitles from all input files
    for input_file in &args.input_files {
        println!("Extracting subtitles from: {:?}", input_file);
        let subtitles = extractor.extract_subtitles(input_file)?;
        all_subtitles.extend(subtitles);
    }

    println!("Found {} subtitle entries", all_subtitles.len());

    // Search for the specified strings
    let time_ranges = search_subtitles(&all_subtitles, &args.search_strings);

    if time_ranges.is_empty() {
        println!(
            "No matches found for any of the search strings: {:?}",
            args.search_strings
        );
        return Ok(());
    }

    println!("Found {} matching time ranges:", time_ranges.len());
    for (i, range) in time_ranges.iter().enumerate() {
        println!(
            "  {}: {:.2}s - {:.2}s from {:?}",
            i + 1,
            range.start,
            range.end,
            range.file_path.file_name().unwrap_or_default()
        );
        println!("      Text: \"{}\"", range.subtitle_text.trim());
    }

    // Create combined video
    println!("Creating combined video...");
    let processor = VideoProcessor;
    processor.create_combined_video(&time_ranges, &output_file, args.audio_only)?;

    println!("Successfully created combined video: {:?}", output_file);
    Ok(())
}
