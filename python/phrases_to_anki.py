deck_name = 'Boku 1'
target_word_count = 1000
subtitle_file = 'boku-1.ass'

import MeCab
import requests
import sys
import genanki
import random
import re

if __name__ != '__main__':
    print('This script is not meant to be imported')
    sys.exit()

def get_japanese_definition(word):
    url = f"https://jisho.org/api/v1/search/words?keyword={word}"
    response = requests.get(url)
    if response.status_code == 200:
        data = response.json()
        if data["data"]:
            count = 0
            meanings = ''
            for meaning in data["data"][0]["senses"]:
                count += 1
                meanings += str(count) + ': ' + '; '.join(meaning["english_definitions"]) + '<br/>'
                if count >= 2:
                    break
            return meanings
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

def is_roman_only(sentence):
    return all(ord(char) < 128 for char in sentence)

def katakana_to_hiragana(text):
    # Create conversion table
    katakana = "ァアィイゥウェエォオカガキギクグケゲコゴサザシジスズセゼソゾタダチヂッツヅテデトドナニヌネノハバパヒビピフブプヘベペホボポマミムメモャヤュユョヨラリルレロワヲンヴヵヶ"
    hiragana = "ぁあぃいぅうぇえぉおかがきぎくぐけげこごさざしじすずせぜそぞただちぢっつづてでとどなにぬねのはばぱひびぴふぶぷへべぺほぼぽまみむめもゃやゅゆょよらりるれろわをんゔゕゖ"
    
    # Create conversion dictionary
    convert_dict = dict(zip(katakana, hiragana))
    
    # Convert each character
    return ''.join(convert_dict.get(char, char) for char in text)

with open(subtitle_file, encoding="utf8") as f:
    subtitles = f.read()

parenthesis_regex = re.compile(r'[（(][^（()）]*[)）]')

ass_regex = re.compile(r'Dialogue:.*?,.*?,.*?,.*?,.*?,.*?,.*?,.*?,.*?,(.*)')
ass_braces = re.compile(r'{.*?}')
def get_ass_phrases(file_contents):
    found = ass_regex.findall(subtitles)
    phrases = []
    for f in found:
        f = re.sub(ass_braces, '', f)
        f = re.sub(parenthesis_regex, '', f)
        phrases.append(f)
    return phrases

srt_regex = re.compile(r'-->[\s0-9:,]+?\n([\w\W]+?)\n\n')
def get_srt_phrases(file_contents):
    found = srt_regex.findall(subtitles)
    phrases = []
    for f in found:
        f = re.sub(parenthesis_regex, '\n', f)
        f = re.sub(parenthesis_regex, '\n', f)
        f = re.sub(parenthesis_regex, '\n', f)
        for p in f.split('\n'):
            p = p.strip()
            if len(p) > 0:
                phrases.append(p)
    return phrases

phrases = None
if subtitle_file.lower().endswith('srt'):
    phrases = get_srt_phrases(subtitles)
else:
    phrases = get_ass_phrases(subtitles)

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
hiragana_sets = {}
sentence_sets = {}

for phrase in phrases:
    tagger = MeCab.Tagger()
    tokens = tagger.parse(phrase).splitlines()[:-1]
    for token in tokens:
        token_parts = token.split('\t')
        katakana = token_parts[2]
        hiragana = katakana_to_hiragana(katakana)
        dictionary_form = token_parts[3].split('-')[0]
        token_part_of_speech = token_parts[4]

        if dictionary_form == '為る' or is_roman_only(dictionary_form):
            continue

        excluded_parts = ['助動詞', '助詞', '補助記号', '空白', '接続詞', '感動詞', '記号', '連体詞', '代名詞']
        if  any(token_part_of_speech.startswith(pos) for pos in excluded_parts):
            continue
        
        if not dictionary_form in sentence_sets:
            sentence_sets[dictionary_form] = set()
        sentence_sets[dictionary_form].add('「' + phrase + '」')

        if not dictionary_form in hiragana_sets:
            hiragana_sets[dictionary_form] = set()
        hiragana_sets[dictionary_form].add(hiragana)
        parts_of_speech[dictionary_form] = token_part_of_speech

        if not dictionary_form in words:
            words[dictionary_form] = 1
        else:
            words[dictionary_form] += 1

model = genanki.Model(
    83746180,
    'Japanese Words',
    fields=[
        {'name': 'Kanji'},
        {'name': 'Kana'},
        {'name': 'Definition'},
        {'name': 'Sentences'},
    ],
    templates=[
        {
        'name': 'Japanese Word Card',
        'qfmt': '<span style="font-size: 60px;">{{Kanji}}</span><br/>{{Kana}}',
        'afmt': '{{FrontSide}}<br/><div style="text-align: left;">{{Definition}}</div><br/>{{Sentences}}',
        },
    ], css='.card {text-align: center; font-size: 18px;}')

deck = genanki.Deck(
  random.randrange(1 << 30, 1 << 31),
  deck_name)

sorted_words = sorted(words.items(), key=lambda item: item[1], reverse=True)
word_count = 0
for word, frequency in sorted_words:
    print(str(frequency) + ' occurrences of ' + word)

    try_count = 0
    definition = None
    while definition == None and try_count < 3:
        try_count += 1
        definition = get_japanese_definition(word)
    if definition == None:
        continue

    definition = parts_of_speech[word] + '<br/>' + definition

    sentences = ''
    for sentence in sentence_sets[word]:
        sentences += sentence + '<br/>'
    
    fields = [word, '、'.join(hiragana_sets[word]), definition, sentences]
    print('\n'.join(fields))
    note = genanki.Note(
        model=model,
        fields=fields)
    
    deck.add_note(note)
    word_count += 1
    if word_count >= target_word_count:
        break

print(str(word_count) + ' words')

genanki.Package(deck).write_to_file(deck_name + '.apkg')

