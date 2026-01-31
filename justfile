set shell := ["nu", "-c"]

#[private]
default:
    just --list

render file output choice="pdf":
    cargo run --release -- render {{choice}} -f {{file}} -o {{output}}

input src output choice="pdf":
    cargo run --release -- render {{choice}} -i {{src}} -o {{output}}

convert file output:
    cargo run --release -- convert -f {{file}} -o {{output}}
