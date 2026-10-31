#!/usr/bin/env python3
"""Генерирует конфигурации экспериментов в experiments/configs/.

Каждая серия меняет один параметр относительно базовой конфигурации (BASE), остальные
параметры остаются базовыми. Пути внутри конфигов отсчитываются от каталога самого конфига,
поэтому словарь, модель и каталог результатов заданы относительно experiments/configs/.

Запуск из корня репозитория: python3 experiments/make_configs.py
"""

from pathlib import Path

CONFIGS_DIR = Path(__file__).resolve().parent / "configs"

# Базовая конфигурация: совпадает с config/default.toml, кроме устройств и частоты,
# взятых из проверенного сеанса (PipeWire, 44,1 кГц)
BASE = {
    "input_device": "PipeWire Sound Server",
    "output_device": "PipeWire Sound Server",
    "sample_rate": 44100,
    "channels": 2,
    "sample_format": "F32",
    "ring_capacity_ms": 10000,
    "delay_ms": 5000,
    "max_distance": 1,
    "chunk_size_ms": 100,
    "partial_results": True,
    "min_confidence": 0.0,
    "strategy": "Beep",
    "padding_ms": 50,
}

# Серия: (каталог результатов, параметр, значения)
SERIES = [
    ("A_chunk", "chunk_size_ms", [50, 100, 200, 400, 800]),
    ("B_delay", "delay_ms", [500, 1000, 1500, 2000, 3000, 5000]),
    ("C_partial", "partial_results", [True, False]),
    ("D_theta", "max_distance", [0, 1, 2, 3]),
    ("E_conf", "min_confidence", [0.0, 0.5, 0.7, 0.9]),
]

TEMPLATE = """\
# Эксперимент {name}: серия {series}, {param} = {value_toml}
# Сгенерировано experiments/make_configs.py — правьте генератор, а не этот файл
[audio]
input_device = "{input_device}"
output_device = "{output_device}"
sample_rate = {sample_rate}
channels = {channels}
sample_format = "{sample_format}"

[buffer]
ring_capacity_ms = {ring_capacity_ms}
delay_ms = {delay_ms}

[decision]
words_dict_path = "../banned_words.json"
max_distance = {max_distance}

[framing]
chunk_size_ms = {chunk_size_ms}

[recognition]
model_path = "../../models/vosk-model-ru-0.22"
partial_results = {partial_results}
min_confidence = {min_confidence}

[masking]
strategy = "{strategy}"
beep_frequency_hz = 1000
amplitude = 0.3
attenuation_db = -30
padding_ms = {padding_ms}

[control]
enabled = true
socket_path = ""

[logging]
# debug: в журнал пишутся все распознанные слова — по ним считаются TP и FP
level = "debug"
dir = "../results/{series}"

[metrics]
enabled = true
export_interval_ms = 10000
dir = "../results/{series}"
"""


def toml_value(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    return str(value)


def suffix(param, value):
    # Имя файла должно сортироваться по значению параметра
    if param == "partial_results":
        return "on" if value else "off"
    if param == "min_confidence":
        return f"{round(value * 100):03d}"
    return f"{value:04d}" if param in ("chunk_size_ms", "delay_ms") else str(value)


def main():
    CONFIGS_DIR.mkdir(parents=True, exist_ok=True)
    for old in CONFIGS_DIR.glob("*.toml"):
        old.unlink()

    for series, param, values in SERIES:
        for value in values:
            name = f"{series}_{suffix(param, value)}"
            params = dict(BASE, **{param: value})
            fields = {k: toml_value(v) for k, v in params.items()}
            text = TEMPLATE.format(
                name=name,
                series=series,
                param=param,
                value_toml=toml_value(value),
                **fields,
            )
            (CONFIGS_DIR / f"{name}.toml").write_text(text, encoding="utf-8")
            print(CONFIGS_DIR.name + "/" + name + ".toml")


if __name__ == "__main__":
    main()
