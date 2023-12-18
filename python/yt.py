import requests
import re
import os
import subprocess
import sys
from pathlib import Path

initialize = False

abspath = os.path.abspath(__file__)
dname = os.path.dirname(abspath)
archive_path = os.path.join(dname, 'yt_download_archive.txt')

downloads_path = str(Path.home() / "Downloads")
os.chdir(downloads_path)

channels = [
    'tested',
    'AbroadinJapan',
    'HealthyGamerGG',
    'mcolville',
    'NASA',
    'smartereveryday',
    'TechnologyConnections',
    'Wintergatan',
    'thegrumps',
    'veritasium',
    'MarcusHouse',
    'scottmanley',
    'TomScottGo',
    'jameshoffmann',
    'ICHIKEN1',
    'EverydayAstronaut',
    'SpaceX',
    'kaguiro',
    'bbcearth',
    'pbsspacetime',
    'Radiohead',
    'SteveMould',
    'CuriosityShow',
    'strandbeestfilm',
    'gendesign4714',
    'Einzelganger',
    'StuffMadeHere',
    'theklainc.5724',
    'StarTalk',
    'CosmicPerspective',
    'StrangeParts',
    'helloMCDM'
]

def get_video_ids(channel):
    channel_url = 'https://www.youtube.com/@' + channel + '/videos'
    html = str(requests.get(channel_url).content)
    matches = re.findall(r'"videoId":"(.*?)"', html)
    return set(matches)

if initialize:
    with open(archive_path, 'w') as f:
        for channel in channels:
            video_ids = get_video_ids(channel)
            for video_id in video_ids:
                f.write('youtube ' + video_id + '\n')
    sys.exit()

with open(archive_path) as f:
    download_archive = f.read()

for channel in channels:
    ids = get_video_ids(channel)
    for video_id in ids:
        if video_id in download_archive:
            continue
        video_url = 'https://www.youtube.com/watch?v=' + video_id
        subprocess.run([
            'yt-dlp',
            '--download-archive',
            archive_path,
            '--format',
            '[height<1440]',
            '--limit-rate',
            '3M',
            '-o',
            '%(channel)s - %(title)s.%(ext)s',
            video_url
        ])
        break

        