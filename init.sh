#!/bin/sh

for hook in .hooks/*; do
    cp "$hook" .git/hooks/
    chmod +x ".git/hooks/$(basename "$hook")"
    echo "✅ Installed hook: $(basename "$hook")"
done