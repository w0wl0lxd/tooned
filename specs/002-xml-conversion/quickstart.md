# Quickstart: XML Support in tooned

## Install

```bash
cargo install tooned-cli
# or a prebuilt binary from the GitHub release, once published
```

## Try XML standalone

```bash
# Convert an XML file only if TOON is smaller than compact JSON
tooned convert config.xml

# Force XML detection even if the extension is misleading
tooned convert data --format xml

# Check whether an XML payload would convert, without producing output
tooned check feed.xml

# Pipe an XML command through the adaptive converter
curl -s https://example.com/api?format=xml | tooned pipe

# Wrap a command whose XML output should be adaptively compacted
tooned wrap -- curl -s https://example.com/api?format=xml
```

## Use from an agent session

If you already have `tooned` installed as a Claude Code or Codex CLI hook, XML tool output will be handled automatically when the XML detection path recognizes it. No additional hook configuration is required.

## What to expect

- XML that is attribute-heavy or record-list-like (a root element with repeated, similarly-shaped child elements) is most likely to convert to smaller TOON.
- Mixed-content XML (e.g., XHTML, text-heavy RSS descriptions) usually passes through unchanged because the JSONified representation is not smaller.
- Malformed or HTML-like content is passed through unchanged.

## Uninstall

XML support is part of the `tooned` binary; uninstall the binary as usual:

```bash
cargo uninstall tooned-cli
```
