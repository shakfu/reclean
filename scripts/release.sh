cargo build --release
install -D target/release/reclean "${PREFIX:-$HOME/.local}/bin/reclean"
