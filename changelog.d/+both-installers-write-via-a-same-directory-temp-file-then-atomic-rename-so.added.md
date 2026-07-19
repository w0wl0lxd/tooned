Both installers write via a same-directory temp-file-then-atomic-rename, so a concurrent installer run never observes a partially-written config file.
