import random

def is_critical(roll):
    return roll == 0 or roll == 11 or roll == 22 or roll == 33 or roll == 44 or roll == 55 or roll == 66 or roll == 77 or roll == 88 or roll == 99

def roll(skill):
    raw_roll = random.randint(0, 99)
    crit = is_critical(raw_roll)
    if raw_roll < skill:
        # success
        if crit:
            return 3
        else:
            return 2
    else:
        # failure
        if crit:
            return 0
        else:
            return 1

wins = 0
losses = 0
draws = 0

for i in range(10000):
    a = roll(20)
    b = roll(80)
    if a > b:
        wins += 1
    elif a < b:
        losses += 1
    else:
        assert(a == b)
        draws += 1

assert(wins + losses + draws == 10000)
print('wins: ' + str(wins / 100))
print('losses: ' + str(losses / 100))
print('draws: ' + str(draws / 100))