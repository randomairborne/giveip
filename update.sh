#!/bin/bash
git pull
RUSTFLAGS="-Ctarget-cpu=native --emit=asm" cargo build --release
sudo systemctl daemon-reload
sudo systemctl restart ip.mcfix.org.service
