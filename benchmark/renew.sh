#! /bin/sh

rm -f benchmark/benchmark_report.html benchmark/history.json

python3 benchmark/run.py run ./mothership/jlox --label "Official Java Lox"
python3 benchmark/run.py run ./mothership/clox --label "Official C Lox"
python3 benchmark/run.py run ./target/release/rox --label "Rust Lox"
