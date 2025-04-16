#!/bin/bash
USAGE="usage: $0 y|n <pat> <file>\n\nalias for: grep -l|-L <pat> <file> &>/dev/null\n\nwhere return code is inverted when -L is passed\n"
[ $# -lt 3 ] && printf "$USAGE" && exit 3
flag=$1
pat=$2
file=$3
invert=false
case "$flag" in
  y) flag=-l ;;
  n) flag=-L; invert=true ;;
  *) exit 3 ;;
esac
grep "$flag" "$pat" "$file" &>/dev/null
ret=$?
$invert && [ $ret -le 1 ] && exit $(( 1 - $ret ))
exit $ret
