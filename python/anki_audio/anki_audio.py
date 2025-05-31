# ffmpeg -i "Cardcaptor Sakura 101.mp4" -map 0:s:0 subs.srt
# ffmpeg -i "Cardcaptor Sakura 101.mp4" -vn -acodec copy output-audio.m4a

# source venv/bin/activate
# python3.12 anki_audio.py audio.wav subs.srt --deck_name "CCS 101"

import os
import uuid
from pathlib import Path
from pydub import AudioSegment
import pysubs2
import genanki
import random
import re

parenthesis_regex = re.compile(r'[（(][^（()）]*[)）]')

def load_subtitles(sub_path):
    return pysubs2.load(sub_path)

def extract_audio_segments(audio_path, subs, output_dir):
    audio = AudioSegment.from_file(audio_path)
    audio_clips = []

    random.shuffle(subs)
    for i, line in enumerate(subs):
        start_ms = int(line.start)
        end_ms = int(line.end) + 300
        
        text = line.text
        text = re.sub(parenthesis_regex, '', text)
        text = re.sub(parenthesis_regex, '', text)
        text = text.strip()
        text = text.replace('\n', '<br/>')
        text = text.replace('\\N', '<br/>')
        if not text or len(text) == 0:
            print(f"Skipping empty subtitle {line.text}")
            continue

        clip = audio[start_ms:end_ms]
        filename = f"clip_{i+1:04}.mp3"
        clip_path = os.path.join(output_dir, filename)
        clip.export(clip_path, format="mp3")
        audio_clips.append((clip_path, text))
    
    return audio_clips

def create_anki_deck(audio_clips, deck_name, output_file):
    deck_id = random.randrange(1, 1 << 31)   # 30-31 bits, safe for SQLite
    model_id = 23978535


    model = genanki.Model(
        model_id,
        f'Audio',
        fields=[{'name': 'Audio'}, {'name': 'Text'}],
        templates=[
            {
                'name': 'Card 1',
                'qfmt': '{{Audio}}',
                'afmt': '{{FrontSide}}<hr id="answer">{{Text}}',
            },
        ],
    )

    deck = genanki.Deck(deck_id, deck_name)
    media_files = []

    for clip_path, text in audio_clips:
        audio_tag = f"[sound:{os.path.basename(clip_path)}]"
        note = genanki.Note(model=model, fields=[audio_tag, text])
        deck.add_note(note)
        media_files.append(clip_path)

    package = genanki.Package(deck)
    package.media_files = media_files
    package.write_to_file(output_file)

def main(audio_file, subtitle_file, output_dir='output', deck_name='Subtitle Audio Deck'):
    os.makedirs(output_dir, exist_ok=True)

    print("Loading subtitles...")
    subs = load_subtitles(subtitle_file)

    print("Extracting audio clips...")
    audio_clips = extract_audio_segments(audio_file, subs, output_dir)

    print("Creating Anki deck...")
    output_apkg = os.path.join(output_dir, f"{deck_name.replace(' ', '_')}.apkg")
    create_anki_deck(audio_clips, deck_name, output_apkg)

    print(f"Anki deck created: {output_apkg}")

if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Generate Anki deck from audio and subtitles")
    parser.add_argument("audio_file", help="Path to audio file")
    parser.add_argument("subtitle_file", help="Path to subtitle file (SRT or ASS)")
    parser.add_argument("--output_dir", default="output", help="Directory to store clips and deck")
    parser.add_argument("--deck_name", default="Subtitle Audio Deck", help="Name of the Anki deck")

    args = parser.parse_args()
    main(args.audio_file, args.subtitle_file, args.output_dir, args.deck_name)
