#!/usr/bin/env python3
"""Print telemetry-parser's flat quat stream as CSV: t_ms,w,x,y,z.

Python twin of o4core/examples/dump_quats.rs (Task 3 spike). Reads
the same GroupId::Quaternion / TagId::Data flat stream in raw emission
order (no sort, no dedup) via mp4patch._flat_reference(), which itself
calls the `telemetry_parser` Python bindings' Parser.telemetry(). Both
outputs must be byte-identical, proving the Rust crate (pinned rev) and
the installed Python package (same underlying crate) agree exactly.

Formatting note: Python's `%.17e` and Rust's `{:.17e}` render the exact
same IEEE-754 double with the exact same 18 significant digits, but with
different exponent conventions -- Python pads to >=2 digits and always
shows a sign (`e+03`, `e-04`); Rust shows no sign for positive exponents
and no padding (`e3`, `e-4`). That is a stdlib text-formatting choice, not
a data difference, so `_rust_exp` below re-renders Python's string into
Rust's convention before printing. Verified against both languages on
{1234.5678, 0.0001234, -0.5, 100.0, 1e-20, -1e20, 0.0, -0.0} -- mantissas
matched bit-for-bit in every case; only the exponent text needed fixing.
"""
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from mp4patch import _flat_reference

_EXP_RE = re.compile(r"^(.*e)([+-])(\d+)$")


def _rust_exp(x, precision=17):
    """Format x like Python's f'{x:.{precision}e}', then rewrite the
    exponent from Python's convention (sign always shown, >=2 digits) to
    Rust's LowerExp convention (sign omitted when positive, no padding)."""
    s = f"{x:.{precision}e}"
    m = _EXP_RE.match(s)
    mantissa, sign, digits = m.group(1), m.group(2), m.group(3)
    digits = digits.lstrip("0") or "0"
    return f"{mantissa}{'-' if sign == '-' else ''}{digits}"


def main():
    video = sys.argv[1]
    ts, qs = _flat_reference(video)
    # Rust's `writeln!` always emits a raw '\n'; force the same here so the
    # diff isn't polluted by Windows' text-mode '\n' -> '\r\n' translation.
    sys.stdout.reconfigure(newline="\n")
    out = sys.stdout
    for t, (w, x, y, z) in zip(ts, qs):
        out.write(
            f"{_rust_exp(t)},{_rust_exp(w)},{_rust_exp(x)},{_rust_exp(y)},{_rust_exp(z)}\n"
        )


if __name__ == "__main__":
    main()
