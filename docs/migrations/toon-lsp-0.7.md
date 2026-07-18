# Migration Guide: toon-lsp 0.7.0 → 0.7.1

This release introduces an in-place, zero-allocation encoding API and a
round-trip verifier. Most existing code continues to compile; the breaking
changes are limited to the `tooned` `Conversion` type (which gains a
lifetime) and a quoting fix for hexadecimal literals.

## toon-lsp API changes

### New: `encode_into(value, config, out)`

`toon_lsp::toon::encode_into` writes the canonical TOON text into an existing
`&mut String` instead of returning a freshly-allocated `String`:

```rust
let mut buf = String::with_capacity(4096);
toon_lsp::toon::encode_into(&value, &ToonConfig::default(), &mut buf)?;
```

The buffer is appended starting at `buf.len()` and is **not** cleared by the
encoder. Call `buf.clear()` yourself when reusing the buffer.

When `fold_keys` and `flatten_keys` are disabled and `buf` has enough capacity,
`encode_into` performs no heap allocations on the encode path.

`encode` and `encode_with_config` are now thin wrappers around `encode_into`
and retain their previous signatures.

### New: `verify_round_trip` / `verify_round_trip_with_scratch`

These functions compare an existing TOON text against the canonical encoding of
an expected `serde_json::Value` without allocating an intermediate `Value`:

```rust
// Allocates a temporary scratch String:
assert!(toon_lsp::toon::verify_round_trip(text, &expected, &config)?);

// Allocation-free if the scratch buffer has capacity:
let mut scratch = String::with_capacity(4096);
assert!(toon_lsp::toon::verify_round_trip_with_scratch(text, &expected, &config, &mut scratch)?);
```

### Hexadecimal literal quoting

`is_toon_number` now recognizes `0x` / `0X` prefixed integers as numeric
literals. This means strings such as `"0x0"` or `"0xDEADBEEF"` are now quoted
during encode so they round-trip correctly. If your application relied on
hex strings being emitted unquoted (which previously produced non-canonical,
non-round-trippable output), you must quote them in the source `Value`.

## tooned `Conversion` lifetime change

`tooned_types::Conversion` is now generic over a lifetime:

```rust
pub enum Conversion<'a> {
    Toon { text: Cow<'a, str>, report: ConversionReport },
    Passthrough { bytes: Cow<'a, [u8]>, reason: PassthroughReason },
}
```

This lets the new in-place functions `toon_from_value` and `maybe_tooned_in`
borrow their output buffers instead of cloning.

- `maybe_tooned` continues to return `Conversion<'static>` (it still clones
  internally for compatibility).
- Use `maybe_tooned_in(input, opts, &mut buf)` or
  `toon_from_value(value, opts, &mut buf)` when you want to avoid the
  allocation.
- If you have code that pattern-matches on `Conversion` without a lifetime,
  add `Conversion<'a>` (or `Conversion<'_>` / `Conversion<'static>` as
  appropriate):

```rust
// before
match conversion {
    Conversion::Toon { text, .. } => text.into_owned(),
    Conversion::Passthrough { bytes, .. } => bytes.into_owned(),
}

// after (when borrowing)
match conversion {
    Conversion::Toon { text, .. } => text.into_owned(),
    Conversion::Passthrough { bytes, .. } => bytes.into_owned(),
}
```

The fields `text` and `bytes` are `Cow`, so `.into_owned()`/`.to_string()`
continue to work exactly as before if you need an owned value.

## Migration checklist

- [ ] Replace hot-path `toon_lsp::toon::encode` / `encode_with_config` calls
      with `encode_into` and a reusable `String` buffer.
- [ ] Replace round-trip checks that parse+compare `Value`s with
      `verify_round_trip_with_scratch` when you can pre-size a scratch buffer.
- [ ] Update any match sites on `tooned_types::Conversion` to
      `Conversion<'a>` / `Conversion<'static>`.
- [ ] Re-encode any fixtures containing unquoted hex strings; the encoder now
      quotes `0x`/`0X` values.
