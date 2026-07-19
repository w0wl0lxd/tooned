**tooned-index:** hardened `.tooned/index.db` and `.gitignore` temp-file paths against symlink redirection by refusing to follow symlinks and using same-directory temp-file-then-rename writes.
