#!/bin/sh
# Differential test of the KD11-B model against simh's PDP-11 as an 11/05.
# Usage: oracle/cputest/run.sh SIMH_PDP11_BINARY [SEED] [COUNT]
set -eu
cd "$(dirname "$0")/../.."
SIMH=$1; SEED=${2:-1}; COUNT=${3:-3000}
D=$(mktemp -d)
python3 -I oracle/cputest/gen.py "$SEED" "$COUNT" "$D/cases.txt"
cargo build -q --release -p kd11b --example cpucheck
./target/release/examples/cpucheck "$D/cases.txt" > "$D/model.txt"
python3 -I oracle/cputest/run_simh.py "$SIMH" "$D/cases.txt" "$D/simh.txt" > /dev/null
python3 -I oracle/cputest/compare.py "$D/model.txt" "$D/simh.txt"
