#!/bin/bash
# Автоматический запуск mykvm и vmm с логированием и ожиданием готовности сокета

set -e

MYKVM_DIR="$(dirname "$0")"
VMM_DIR="$MYKVM_DIR/../vmm"
SOCK="/tmp/mykvm.sock"
LOG="$MYKVM_DIR/mykvm.log"

# Удаляем старый сокет
rm -f "$SOCK"

# Запускаем mykvm в фоне с логом
cd "$MYKVM_DIR"
cargo run > "$LOG" 2>&1 &
MYKVM_PID=$!

# Ждём появления сокета (готовности mykvm)
for i in {1..20}; do
    if [ -S "$SOCK" ]; then
        break
    fi
    sleep 0.2
done
if [ ! -S "$SOCK" ]; then
    echo "[auto_run] mykvm не создал сокет $SOCK, запуск невозможен!"
    kill $MYKVM_PID
    exit 1
fi

echo "[auto_run] mykvm готов, запускаем vmm..."
cd "$VMM_DIR"
cargo run

# После завершения vmm убиваем mykvm
kill $MYKVM_PID
