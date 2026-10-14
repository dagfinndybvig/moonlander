#!/bin/sh
# Assemble the reconstructed Mare Assabet source and check it against the
# surviving binary. Exits non-zero unless the memory image matches exactly.
#
# Needs: macro11 (github.com/Rhialto/macro11) on PATH or in $MACRO11,
#        python3. Outputs go to out/.
set -eu
cd "$(dirname "$0")"
MACRO11=${MACRO11:-macro11}
TOOLS=../oracle/tools
mkdir -p out
"$MACRO11" -rt11 lunar-mare-assabet.mac -o out/lunar.obj -l out/lunar.lst
python3 -I $TOOLS/link.py out/lunar.obj --base 0 \
    --lda out/lunar.lda --bin out/lunar.bin --json out/lunar.json
python3 -I $TOOLS/compare_images.py out/lunar.lda \
    ../reference/original/lunar-later-nosource.lda out/lunar.lst
