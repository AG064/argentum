import sys


def count(text: str) -> tuple[int, int, int]:
    return len(text.splitlines()), len(text.split()), len(text)


def main(argv: list[str]) -> int:
    with open(argv[1], encoding="utf-8") as handle:
        lines, words, chars = count(handle.read())
    print(f"{lines} {words} {chars}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
