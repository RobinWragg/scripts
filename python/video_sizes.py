import os
import sys
import subprocess
import glob
import re
import time

extensions = ['.mkv', '.mp4', '.m4v', '.wmv', '.mov', '.webm', '.flv', '.avi']
os.chdir(sys.argv[1])
file_infos = []

resolution_pattern = re.compile(r',\s+?(\d+)x(\d+)')
max_num_pixels_480p = 665760
max_num_pixels_720p = 1497600

sixty_long = ''

for i in range(60):
	sixty_long += ' '

def shrink_inspection_text(text):
	if len(text) > 60:
		return text[:28] + '...' + text[-28:]
	return text

def show_inspection(text, failure=False):
	print('\r' + sixty_long, end='')
	text = shrink_inspection_text(text)
	
	if failure:
		text = '\rFAILED: ' + text
		print(shrink_inspection_text(text))
	else:
		text = '\rInspecting ' + text
		print(shrink_inspection_text(text), end='')
	
# get
for file_path in glob.iglob('**', recursive=True):
	if os.path.basename(file_path) == '.' or os.path.isdir(file_path):
		continue
	
	extension_match = False
	for extension in extensions:
		if file_path.lower().endswith(extension):
			extension_match = True
			break
	
	if not extension_match:
		continue
	
	show_inspection(file_path)
	
	try:
		popen = subprocess.Popen(['ffprobe', file_path], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
		out, err = popen.communicate()
		duration_str = "Duration: "
		ffprobe_response = err.decode("utf-8")
		duration_index = ffprobe_response.find(duration_str)
		duration_numbers_start_index = duration_index+len(duration_str)
		duration_numbers = ffprobe_response[duration_numbers_start_index:duration_numbers_start_index+8]
		
		hours = int(duration_numbers[0:2])
		minutes = int(duration_numbers[3:5])
		seconds = int(duration_numbers[6:8])
		
		total_seconds = hours*60*60 + minutes*60 + seconds
		
		match = resolution_pattern.search(ffprobe_response)
		
		info = {}
		info['name'] = file_path
		info['kbs'] = os.path.getsize(file_path) / 1024
		info['duration'] = total_seconds
		info['num_pixels'] = int(match.group(1)) * int(match.group(2))
		file_infos.append(info)
	except:
		show_inspection(file_path, True)

file_infos_480p = []
file_infos_720p = []
file_infos_1080p = []

for info in file_infos:
	if info['num_pixels'] <= max_num_pixels_480p:
		file_infos_480p.append(info)
	elif info['num_pixels'] <= max_num_pixels_720p:
		file_infos_720p.append(info)
	else:
		file_infos_1080p.append(info)

# sort
file_infos_480p = sorted(file_infos_480p, key = lambda info: -(info['kbs'] / info['duration']))
file_infos_720p = sorted(file_infos_720p, key = lambda info: -(info['kbs'] / info['duration']))
file_infos_1080p = sorted(file_infos_1080p, key = lambda info: -(info['kbs'] / info['duration']))

# print
def print_infos(info_list, threshold):
	# threshold = 0
	for info in info_list:
		kb_per_sec = info['kbs'] / info['duration']
		if kb_per_sec >= threshold:
			print(info['name'])
			kb_per_sec_str = str(round(kb_per_sec, 2)) + ' KBs per second'
			seconds_str = str(round(info['duration'])) + ' seconds'
			kbs_str = str(round(info['kbs'] / 1024)) + ' MBs'
			print('\t' + kb_per_sec_str + ', ' + kbs_str + ', ' + seconds_str)

print()

print('1080p')
if '-a' in sys.argv:
	print_infos(file_infos_1080p, 0)
else:
	print_infos(file_infos_1080p, 220)
print()

print('720p')
if '-a' in sys.argv:
	print_infos(file_infos_720p, 0)
else:
	print_infos(file_infos_720p, 160)
print()

print('480p')
if '-a' in sys.argv:
	print_infos(file_infos_480p, 0)
else:
	print_infos(file_infos_480p, 140)
print()






