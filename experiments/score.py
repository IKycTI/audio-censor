#!/usr/bin/env python3
"""Считает TP, FP, Precision и Recall запуска по журналу уровня debug.

Берёт окончательные результаты распознавания (строки «Получено N слов: [...]»), прогоняет
каждое слово через те же правила, что и программа (src/decision/rules.rs), для порогов θ = 0–3
и сравнивает с эталоном из ANALYTICS.md: какие словоформы текста считаются запрещёнными.
Частичные гипотезы не учитываются: окончательный результат их подтверждает или отменяет.

Подсчёт строгий: TP — только словоформы эталона. Другая форма запрещённого слова, которой
в тексте нет (распознано «хакера» вместо «хакеров»), считается FP.

Запуск: python3 experiments/score.py experiments/results/<серия>/<сеанс>.log [--min-conf 0.5]
"""

import argparse
import collections
import json
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
SHORTEST_WORD_LENGTH = 4

# Эталон: словоформы текста, которые должны быть заглушены (ANALYTICS.md)
TRUTH_FORMS = {
    "хакер": {"хакер", "хакеры", "хакеров"},
    "злоумышленник": {"злоумышленник", "злоумышленники", "злоумышленников"},
    "утечка": {"утечка", "утечки", "утечке", "утечку"},
    "взлом": {"взлом", "взломали", "взломанная"},
    "украли": {"украли", "украл"},
    "суд": {"суд"},
    "пароль": {"паролем", "паролей"},
    "мошенничество": {"мошенничества"},
    "кофе": {"кофе"},
    "данные": {"данные", "данных", "данным"},
}
TRUTH_TOTAL = 72

WORD_RE = re.compile(r'word: "([^"]*)", start_ms: (\d+), end_ms: (\d+), conf: ([0-9.]+)')


def levenshtein(a, b):
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        cur = [i]
        for j, cb in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (ca != cb)))
        prev = cur
    return prev[-1]


def normalize(word):
    return "".join(c for c in word.lower() if c.isalnum())


def is_bad(word, dictionary, theta):
    if word in dictionary:
        return True
    if theta == 0 or len(word) < SHORTEST_WORD_LENGTH:
        return False
    return any(
        len(b) >= SHORTEST_WORD_LENGTH and levenshtein(word, b) <= theta for b in dictionary
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("log", type=Path)
    parser.add_argument("--min-conf", type=float, default=0.0, help="порог уверенности c_min")
    args = parser.parse_args()

    dictionary = {normalize(w) for w in json.loads((HERE / "banned_words.json").read_text())["words"]}
    all_forms = set().union(*TRUTH_FORMS.values())

    words = []
    for line in args.log.read_text(encoding="utf-8").splitlines():
        if "Получено" in line:
            words += [(normalize(w), float(c)) for w, _, _, c in WORD_RE.findall(line)]
    print(f"Распознано слов (окончательные результаты): {len(words)}")

    found_forms = collections.Counter(w for w, _ in words if w in all_forms)
    print(f"Словоформ эталона среди распознанных: {sum(found_forms.values())} из {TRUTH_TOTAL}")

    print(f"\nc_min = {args.min_conf}")
    print("θ  TP  FP  FN  Precision  Recall  F1")
    for theta in range(4):
        hits = [w for w, c in words if c >= args.min_conf and is_bad(w, dictionary, theta)]
        tp = sum(1 for w in hits if w in all_forms)
        fp = len(hits) - tp
        fn = TRUTH_TOTAL - tp
        p = tp / len(hits) if hits else 0.0
        r = tp / TRUTH_TOTAL
        f1 = 2 * p * r / (p + r) if p + r else 0.0
        print(f"{theta}  {tp:2d}  {fp:2d}  {fn:2d}  {p:9.3f}  {r:6.3f}  {f1:.3f}")
        false = collections.Counter(w for w in hits if w not in all_forms)
        if false:
            print("   ложные:", ", ".join(f"{w} {n}" for w, n in false.most_common()))

    missed = {f: n for lemma, forms in TRUTH_FORMS.items() for f in forms if (n := found_forms[f]) >= 0}
    print("\nРаспознано словоформ эталона:", ", ".join(f"{f} {n}" for f, n in sorted(missed.items())))


if __name__ == "__main__":
    main()
