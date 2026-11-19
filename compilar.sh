#!/bin/sh
# Compila el Reductor de sólidos. Requisitos: Python 3, Rust y, para el .exe, MinGW-w64 (g++-mingw-w64-x86-64-posix).
#   ./compilar.sh           → Reductor_solidos.exe (Windows) en la raíz
#   ./compilar.sh linux     → motor/target/release/Reductor_solidos (pruebas: ./Reductor_solidos --servidor)
set -e
cd "$(dirname "$0")"
python3 red/build.py
cd motor
if [ "$1" = "linux" ]; then
  cargo build --release
else
  CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc-posix AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar \
    cargo build --release --target x86_64-pc-windows-gnu
  cp target/x86_64-pc-windows-gnu/release/Reductor_solidos.exe ../Reductor_solidos.exe
  ls -l ../Reductor_solidos.exe
fi
