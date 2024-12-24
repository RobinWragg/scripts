deck_name = 'K-On Vocabulary'
furigana = False
target_word_count = 500

import MeCab
import requests
import sys
import genanki
import random

if __name__ != '__main__':
    print('This script is not meant to be imported')
    sys.exit()

def get_japanese_definition(word):
    url = f"https://jisho.org/api/v1/search/words?keyword={word}"
    response = requests.get(url)
    if response.status_code == 200:
        data = response.json()
        if data["data"]:
            for meaning in data["data"][0]["senses"]:
                return '; '.join(meaning["english_definitions"])
    print('No definition found for ' + word)
    return None

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

def katakana_to_hiragana(text):
    # Create conversion table
    katakana = "ァアィイゥウェエォオカガキギクグケゲコゴサザシジスズセゼソゾタダチヂッツヅテデトドナニヌネノハバパヒビピフブプヘベペホボポマミムメモャヤュユョヨラリルレロワヲンヴヵヶ"
    hiragana = "ぁあぃいぅうぇえぉおかがきぎくぐけげこごさざしじすずせぜそぞただちぢっつづてでとどなにぬねのはばぱひびぴふぶぷへべぺほぼぽまみむめもゃやゅゆょよらりるれろわをんゔゕゖ"
    
    # Create conversion dictionary
    convert_dict = dict(zip(katakana, hiragana))
    
    # Convert each character
    return ''.join(convert_dict.get(char, char) for char in text)

with open('phrases.txt', 'r', encoding='utf-8') as f:
    phrases = f.readlines()

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

parts_of_speech = {}
hiragana_forms = {}

for text in phrases:
    tagger = MeCab.Tagger()
    tokens = tagger.parse(text).splitlines()[:-1]
    for token in tokens:
        token_parts = token.split('\t')
        katakana = token_parts[2]
        hiragana = katakana_to_hiragana(katakana)
        dictionary_form = token_parts[3].split('-')[0]
        token_part_of_speech = token_parts[4]

        if dictionary_form == '為る':
            continue
        
        hiragana_forms[dictionary_form] = hiragana

        processed_part_of_speech = None
        if token_part_of_speech.startswith(noun):
            processed_part_of_speech = 'noun'
        elif token_part_of_speech.startswith(verb):
            processed_part_of_speech = 'verb'
        elif token_part_of_speech.startswith(adjective):
            processed_part_of_speech = 'adjective'
        
        if processed_part_of_speech != None:
            if dictionary_form in words:
                words[dictionary_form] += 1
            else:
                parts_of_speech[dictionary_form] = processed_part_of_speech
                words[dictionary_form] = 1

model = genanki.Model(
    random.randrange(1 << 30, 1 << 31),
    'Simple Model',
    fields=[
        {'name': 'Question'},
        {'name': 'Answer'},
    ],
    templates=[
        {
        'name': 'Card 1',
        'qfmt': '<span style="font-size: 50px;">{{Question}}</span>',
        'afmt': '{{FrontSide}}<hr id="answer"><span style="font-size: 30px;">{{Answer}}</span>',
        },
    ], css='.card {text-align: center;}')

deck = genanki.Deck(
  random.randrange(1 << 30, 1 << 31),
  deck_name)

sorted_words = sorted(words.items(), key=lambda item: item[1], reverse=True)
word_count = 0
for word, frequency in sorted_words:
    print(f'{word}: {frequency}')
    definition = get_japanese_definition(word)
    if definition == None:
        continue
    
    note = None
    if furigana:
        note = genanki.Note(
            model=model,
            fields=[word + '<br/><span style="font-size: 16px;">' + hiragana_forms[word] + '</span>', definition])
    else:
        note = genanki.Note(
            model=model,
            fields=[word, hiragana_forms[word] + '<br/>' + definition])
    
    deck.add_note(note)
    word_count += 1
    if word_count >= target_word_count:
        break

genanki.Package(deck).write_to_file(deck_name + '.apkg')

