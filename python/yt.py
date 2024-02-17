import grequests
import re
import sys
import os
import json
import webbrowser

initialize = False

abspath = os.path.abspath(__file__)
dname = os.path.dirname(abspath)
no_show_list_path = os.path.join(dname, '..', 'yt_no_show_list.txt')

os.chdir(dname)

channels = [
    'SSkorkowsky',
    'tested',
    'AbroadinJapan',
    'HealthyGamerGG',
    'kylehill',
    'mcolville',
    'NASA',
    'smartereveryday',
    'TechnologyConnections',
    'GameGrumps',
    'Wintergatan',
    'thegrumps',
    'veritasium',
    'MarcusHouse',
    'scottmanley',
    'TomScottGo',
    'kanamenaito',
    'jameshoffmann',
    'BPSspace',
    'ICHIKEN1',
    'EverydayAstronaut',
    'SpaceX',
    'kaguiro',
    'AdamNeely',
    'pbsspacetime',
    'Radiohead',
    'SteveMould',
    'strandbeestfilm',
    'gendesign4714',
    'Einzelganger',
    'StuffMadeHere',
    'theklainc.5724',
    'StarTalk',
    'CosmicPerspective',
    'StrangeParts',
    'helloMCDM',
    '_noisecode',
    'Onomappu',
    'SciShow',
    'JonTronShow',
    'Mystery_Quest',
    'TheGlassCannon'
]

def get_response_json(response):
    html = response.content.decode('utf-8')
    j = re.search(r'var ytInitialData = (.*?);</script>', html)
    j = j[1]
    j = json.loads(j)
    return j

channel_requests = []

for channel in channels:
    channel_url = 'https://www.youtube.com/@' + channel + '/videos'
    req = grequests.get(channel_url)
    channel_requests.append(req)

channel_responses = grequests.map(channel_requests)

all_videos = []

for c in range(len(channels)):
    channel = channels[c]
    response = channel_responses[c]

    j = get_response_json(response)
    try:
        tabs = j['contents']['twoColumnBrowseResultsRenderer']['tabs']
        videos = tabs[1]['tabRenderer']['content']['richGridRenderer']['contents']
        for v in videos:
            if 'richItemRenderer' in v:
                videoId = v['richItemRenderer']['content']['videoRenderer']['videoId']
                title = v['richItemRenderer']['content']['videoRenderer']['title']['runs'][0]['text']
                all_videos.append((videoId, channel, title))
    except KeyError:
        print('KeyError')
        print(j)

with open(no_show_list_path, 'r') as f:
    no_show_list = f.read()

filtered_videos = {}

count = 0
for video in all_videos:
    if not video[0] in no_show_list:
        filtered_videos[count] = video
        count += 1

print('Vids:')
for k in filtered_videos.keys():
    video = filtered_videos[k]
    print(str(k) + ' ' + video[1] + '\t' + video[2])

print('> ', end='')
user_inputs = input().split()

to_opens = []
to_removes = []
for input_str in user_inputs:
    should_open = False
    index = 0
    try:
        index = int(input_str)
        should_open = True
    except:
        try:
            index = int(input_str[1:])
        except:
            print('ERROR! ' + input_str)
            sys.exit()
    if should_open:
        to_opens.append(filtered_videos[index])
    to_removes.append(index)

for to_open in to_opens:
    webbrowser.open('https://www.youtube.com/watch?v=' + to_open[0])

for to_remove in to_removes:
    del filtered_videos[to_remove]

new_no_show_list = ''
for vid in all_videos:
    found = False
    for filtered_vid in filtered_videos:
        if vid[0] == filtered_videos[filtered_vid][0]:
            found = True
            break
    if not found:
        new_no_show_list += vid[0] + '\n'

with open(no_show_list_path, 'w') as f:
    f.write(new_no_show_list)
