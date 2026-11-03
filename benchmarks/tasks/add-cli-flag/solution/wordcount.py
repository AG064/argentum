import json
import sys


def count(text: str) -> tuple[int, int, int]:
    return len(text.splitlines()), len(text.split()), len(text)


def main(argv: list[str]) -> int:
    arguments = [value for value in argv[1:] if value != "--json"]
    with open(arguments[0], encoding="utf-8", newline="") as handle:
        lines, words, chars = count(handle.read())
    if "--json" in argv[1:]:
        print(json.dumps({"lines": lines, "words": words, "chars": chars}))
    else:
        print(f"{lines} {words} {chars}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
