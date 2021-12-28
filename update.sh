#!/bin/bash
git fetch
git reset --hard origin/master
chmod +x update.sh
RUSTFLAGS="-Ctarget-cpu=native --emit=asm" cargo build --release
sudo systemctl daemon-reload
sudo systemctl restart ip.mcfix.org.service
