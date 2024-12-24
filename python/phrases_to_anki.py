import MeCab
import requests
import sys

if __name__ != '__main__':
    print('This script is not meant to be imported')
    sys.exit()

def get_jlpt_level_from_jisho(word):
    url = f"https://jisho.org/api/v1/search/words?keyword={word}"
    response = requests.get(url)
    if response.status_code == 200:
        data = response.json()
        if data['data']:
            for entry in data['data']:
                jlpt = entry.get('jlpt', [])
                if jlpt:
                    return jlpt
        return "No JLPT level found"
    else:
        return f"Error: {response.status_code}"

with open('phrases.txt', 'r', encoding='utf-8') as f:
    phrases = f.readlines()

# phrases = phrases[0:1000]

words = {}

# Parts of speech (品詞, ひんし)
noun = '名詞' # めいし
verb = '動詞' # どうし
adjective = '形容詞' # けいようし
adverb = '副詞' # ふくし
adnominal = '連体詞' # れんたいし
particle = '助詞' # じょし
conjunction = '接続詞' # せつぞくし
interjection = '感動詞' # かんどうし
prefix = '接頭詞' # せっとうし
suffix = '接尾詞' # せつびし
auxiliary = '助動詞' # じょどうし
others = 'その他' # そのた

for text in phrases:
    tagger = MeCab.Tagger()
    tokens = tagger.parse(text).splitlines()[:-1]
    for token in tokens:
        token_parts = token.split('\t')
        dictionary_form = token_parts[3].split('-')[0]
        part_of_speech = token_parts[4]
        
        if part_of_speech.startswith(noun) or part_of_speech.startswith(verb) or part_of_speech.startswith(adjective):
            if dictionary_form in words:
                words[dictionary_form] += 1
            else:
                words[dictionary_form] = 1

print('Most common 500 nouns, verbs and adjectives:')
sorted_words = sorted(words.items(), key=lambda item: item[1], reverse=True)
word_count = 0
for word, frequency in sorted_words:
    print(f'{word}: {frequency}')
    word_count += 1
    if word_count >= 500:
        break
