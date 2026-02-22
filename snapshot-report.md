# Snapshot Test Review Report

## Finding 1: Misleading error for `newline_after_equal` (minor)

**File:** `parser__bad_keys__newline_after_equal.snap`

**Input:** `key=\n3`

**Snapshot error:** `error[invalid-number]: unable to parse number`

The parser reports "unable to parse number" when the actual issue is a missing value / unexpected newline after `=`. Compare with the `eof` test (`key =` with no value and no newline) which correctly reports `error[unexpected-eof]: unexpected end of file`. The `newline_after_equal` case should ideally produce a similar "expected a value" or "unexpected newline" error rather than falling through to the number parser.

Not a correctness bug (the input is correctly rejected), but users would get a confusing diagnostic.

---

## Finding 2: Misleading "non-table" label in `table_9_reverse_invalid`

**File:** `parser__table_9_reverse_invalid.snap`

**Input:**
```toml
[fruit.apple.taste]  # defines taste as a table

[fruit]
apple.color = "red"
apple.taste.sweet = true  # tries to extend taste via dotted key
```

**Snapshot error:**
```
error[dotted-key-invalid-type]: dotted key attempted to extend non-table type
  ┌─ table_9_reverse_invalid:6:7
  │
2 │ [fruit.apple.taste]  # INVALID
  │              ----- non-table
  ·
6 │ apple.taste.sweet = true
  │       ^^^^^ attempted to extend table here
```

The label "non-table" on `taste` is wrong — `[fruit.apple.taste]` explicitly defines `taste` as a table. The real issue is that the table is "frozen" (sealed by the explicit header) and cannot be extended via dotted keys. Compare with the forward-direction test `table_9_invalid`, which correctly reports `error[duplicate-key]` for the same semantic problem. The reverse direction should ideally use a consistent error kind.

---

## Finding 3: Invalid datetimes reported as `invalid-number` (minor)

**Files:** `parser__datetimes__utc_invalid.snap`, `parser__datetimes__utc_trailing_dot.snap`, `parser__datetimes__tz2.snap`, `parser__datetimes__tz_neg2.snap`

All four invalid datetime inputs are reported as `error[invalid-number]: unable to parse number`:

| Test | Input | Actual issue |
|---|---|---|
| `utc_invalid` | `2016-9-09T09:09:09Z` | Single-digit month (`9` not `09`) |
| `utc_trailing_dot` | `2016-09-09T09:09:09.Z` | Trailing dot in fractional seconds |
| `tz2` | `2016-09-09T09:09:09+2:00` | Single-digit offset hour (`+2:00` not `+02:00`) |
| `tz_neg2` | `2016-09-09T09:09:09-2:00` | Single-digit offset hour (`-2:00` not `-02:00`) |

These inputs are recognizably datetimes (starting with `YYYY-`, containing `T`), but the parser falls through to the number parser and reports "unable to parse number" instead of a datetime-specific error. Same class of misleading diagnostic as Finding 1. Not a correctness bug — all inputs are correctly rejected.

Note: `tz_neg3` (`2016-09-09T09:09:09Z-2:00`) is fine — it correctly parses up to `Z` and reports "expected newline" for the trailing `-2:00`.

---

## Finding 4: Inconsistent error span widths for bad floats (cosmetic)

Various `bad_floats` error snapshots have inconsistent span widths:

- `trailing_dec` (`a = 0.`): highlights `^` at the trailing `.` only
- `trailing_exp` (`a = 0.e`): highlights `^` at `0` only (1 char)
- `trailing_exp3` (`a = 0.0E`): highlights `^` at `0` only (1 char)
- `underscore_before_dot` (`a = 73_.5`): highlights `^^^` covering `73_` (3 chars)
- `underscore_after_exp` (`a = 73e_3`): highlights `^^^^^` covering `73e_3` (5 chars)

The parser sometimes highlights only the first character and sometimes the full invalid token. This is a cosmetic inconsistency in error reporting, not a correctness issue.

---

## No Issues Found In

Everything else I reviewed looks correct:

- **Integers:** All values match inputs including hex (`0xDEADBEEF` → 3735928559), octal (`0o01234567` → 342391), binary (`0b11010110` → 214), and boundary values (i64::MAX, i64::MIN).
- **Floats:** All computed values are correct, including underscore-separated numbers (`2_0.1_0e1_0` → 201000000000.0).
- **Strings:** All escape sequences verified including `\u`, `\U`, `\x` (TOML 1.1), `\e` (TOML 1.1), line-ending backslash, literal strings preserving backslashes, and multi-line string first-newline trimming.
- **Datetimes:** Correct handling of UTC, timezone offsets, fractional seconds, lowercase `t`/`z` normalization, space delimiter normalization to `T`, and TOML 1.1 no-seconds forms (filled to `:00`).
- **Tables:** Correct nesting, implicit groups, array-of-tables, dotted keys, duplicate detection, and all invalid nesting cases.
- **Inline tables:** Correct parsing of empty, nested, and multi-value inline tables. TOML 1.1 features (newlines, trailing commas, comments inside inline tables) all work correctly.
- **Deserialization:** `basic_table`, `basic_arrays`, `flattened`, `missing_required`, `unknown_field`, and `custom_error` snapshots all match expected behavior.
- **Span positions:** Verified multiple span snapshots — byte offsets and widths are accurate for strings, integers, floats, arrays, tables, and nested structures.
- **Boolean errors:** Correct detection and messaging for `true2`, `false2`, `t2`, `f2`.
- **Leading zeros:** Correctly rejected for all combinations (`00`, `+00`, `-00`, `00.0`, etc.).
- **Underscore rules:** Correctly rejected for trailing, double, and leading underscores.
- **Key validation:** Correct rejection of multiline string keys, empty keys, pipe in keys, newlines in quoted keys, CR in strings.
- **Table name validation:** Correct rejection of empty, period-only, exclamation, trailing period, newline, unterminated, and multiline string table names.
