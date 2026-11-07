#!/usr/bin/env -S uv run
# /// script
# dependencies = ["ofxtools"]
# ///

"""Copy dates found in OFX memos into DTUSER without rewriting the OFX file."""

from __future__ import annotations

import argparse
import io
import re
import sys
from datetime import datetime
from pathlib import Path

from ofxtools.header import OFXHeaderError, parse_header


DATE_PATTERNS = (
    re.compile(r"\b(\d{2})\.(\d{2})\.(\d{2}|\d{4})\b"),
    re.compile(r"\b(\d{2})/(\d{2})/(\d{2}|\d{4})\b"),
)
STMTTRN_PATTERN = re.compile(
    r"<STMTTRN\b[^>]*>.*?(?:</STMTTRN>|(?=</(?:BANKTRANLIST|CCSTMTTRN|INVTRANLIST)>|<STMTTRN\b)|\Z)",
    re.IGNORECASE | re.DOTALL,
)


def extract_date(text: str) -> datetime | None:
    """Extract a date written as DD.MM.YY or DD/MM/YY from *text*."""
    for pattern in DATE_PATTERNS:
        match = pattern.search(text)
        if match is None:
            continue
        day, month, year = map(int, match.groups())
        if year < 100:
            year += 2000 if year < 70 else 1900
        try:
            return datetime(year, month, day)
        except ValueError:
            return None
    return None


def field(block: str, name: str) -> str:
    """Return the text of an OFX leaf field, supporting SGML and XML."""
    match = re.search(
        rf"<{name}>\s*([^<]*?)(?:</{name}>|(?=<))",
        block,
        re.IGNORECASE,
    )
    return match.group(1).strip() if match else ""


def set_dtuser(block: str, value: str) -> str:
    """Replace or insert DTUSER in one STMTTRN block."""
    element = f"<DTUSER>{value}</DTUSER>"
    existing = re.compile(
        r"<DTUSER>\s*[^<]*?(?:</DTUSER>|(?=<))",
        re.IGNORECASE,
    )
    if existing.search(block):
        return existing.sub(element, block, count=1)

    posted = re.compile(
        r"(<DTPOSTED>\s*[^<]*?(?:</DTPOSTED>|(?=<)))",
        re.IGNORECASE,
    )
    return posted.sub(rf"\1{element}", block, count=1)


def update_transaction_dates(source: str) -> tuple[str, int]:
    """Add extracted dates to DTUSER and return the updated text and count."""
    updated = 0

    def replace(match: re.Match[str]) -> str:
        nonlocal updated
        block = match.group(0)
        memo = field(block, "MEMO") or field(block, "NAME")
        date = extract_date(memo)
        if date is None:
            return block
        updated += 1
        return set_dtuser(block, date.strftime("%Y%m%d"))

    return STMTTRN_PATTERN.sub(replace, source), updated


def output_path(path: Path) -> Path:
    """Return the default output path for an input OFX file."""
    return path.with_name(f"{path.stem}-modified{path.suffix or '.ofx'}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("file", type=Path, help="OFX file to update")
    parser.add_argument("-o", "--output", type=Path, help="output OFX file")
    args = parser.parse_args()

    input_path: Path = args.file
    output: Path = args.output or output_path(input_path)
    raw = input_path.read_bytes()

    try:
        header, _ = parse_header(io.BytesIO(raw))
        text = raw.decode(header.codec)
    except (OSError, OFXHeaderError, UnicodeDecodeError) as error:
        print(f"OFX read error: {error}", file=sys.stderr)
        return 1

    modified, count = update_transaction_dates(text)
    output.write_bytes(modified.encode(header.codec))
    print(f"{count} transaction(s) updated")
    print(f"File written: {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
