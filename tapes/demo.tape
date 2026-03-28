# Record: vhs tapes/demo.tape
# Prereq: daylog init && daylog sync (need populated demo data)

Output tapes/demo.gif
Require daylog
Require jq
Set Shell fish
Set FontSize 15
Set Width 900
Set Height 550
Set Padding 20
Set Theme "Catppuccin Mocha"
Set TypingSpeed 40ms

Sleep 500ms

Type "daylog log weight 173.4"
Enter
Sleep 700ms

Type "daylog log sleep 10:30pm-6:15am"
Enter
Sleep 700ms

Type "daylog log mood 4"
Enter
Sleep 700ms

Type "daylog log lift squat 185x5, 205x3, 225x1"
Enter
Sleep 700ms

Type "daylog log metric resting_hr 52"
Enter
Sleep 1s

Type "daylog status | jq .today"
Enter
Sleep 4s
