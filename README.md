# audio-censor

Демон для Linux и Windows: захватывает живой звук, распознаёт русскую речь с помощью
[Vosk](https://alphacephei.com/vosk/) и маскирует запрещённые слова до того, как они
попадут на устройство вывода. Звук выводится с задержкой `buffer.delay_ms`, чтобы
распознавание успело найти слово раньше, чем оно прозвучит.

## Зависимости, не входящие в репозиторий

Нативная библиотека Vosk и модель не хранятся в git (большие платформозависимые файлы),
их нужно получить до сборки.

### `libvosk.so`

Библиотека берётся из Python-колеса `vosk` с PyPI, его скачивает [uv](https://docs.astral.sh/uv/)
(версия закреплена в `pyproject.toml` и `uv.lock`):

```sh
uv sync
```

Python нужен только для этого шага, программе во время работы он не нужен. Без библиотеки
не собирается ничего, включая `cargo test`. `build.rs` ищет её по порядку: каталог из переменной
`VOSK_LIB_DIR` → пакет `vosk` в `.venv` → `lib/` (старая схема: библиотека из релизов
[vosk-api](https://github.com/alphacep/vosk-api/releases), положенная вручную). Затем копирует
её в `target/<profile>/`, рядом с бинарником: он находит её сам (rpath `$ORIGIN`).

На Windows колесо содержит `libvosk.dll` и рантайм MinGW (`libgcc_s_seh-1.dll`, `libstdc++-6.dll`,
`libwinpthread-1.dll`), но не import library, которая нужна для линковки. `build.rs` сам
собирает её из экспортов DLL (`libvosk.lib` для MSVC, `libvosk.dll.a` для GNU) и копирует все
четыре DLL рядом с `audio-censor.exe`. Пошаговый запуск — в разделе [Запуск на Windows](#запуск-на-windows).

### `models/vosk-model-ru-0.22`

```sh
mkdir -p models && cd models
wget https://alphacephei.com/vosk/models/vosk-model-ru-0.22.zip
unzip vosk-model-ru-0.22.zip
```

Путь к модели задаётся в конфиге: `recognition.model_path`.

## Сборка и запуск

Без `-c` конфиг ищется по порядку, берётся первый найденный (какой — пишется в лог):

1. `~/.config/audio-censor/config.toml` (точнее `$XDG_CONFIG_HOME/audio-censor/config.toml`;
   на Windows — `%APPDATA%\audio-censor\config.toml`) — личные настройки;
2. `config/default.toml` от текущего каталога — запуск из корня проекта (`cargo run`);
3. `config/default.toml` рядом с бинарником — распакованная поставка.

Пути внутри конфига (словарь, модель, логи, метрики, сокет) отсчитываются от каталога самого
конфига. В `~/.config/audio-censor/config.toml` путь к модели удобнее писать абсолютным.

```sh
cargo build --release
cargo run -- --show-device all        # список устройств (input | output | all)
cargo run                             # демон: ~/.config/audio-censor/config.toml или config/default.toml
cargo run -- -c config/config.toml    # свой конфиг (config/config.toml не хранится в git)
cargo test
```

Управление запущенным демоном (через Unix-сокет, по умолчанию
`$XDG_RUNTIME_DIR/audio-censor.sock`; на Windows — именованный канал
`\\.\pipe\audio-censor-<пользователь>`):

```sh
cargo run -- status          # метрики в JSON
cargo run -- reload-dict     # перечитать словарь без перезапуска
cargo run -- save-metrics    # промежуточный снимок метрик в файл сеанса
cargo run -- shutdown        # корректная остановка (как Ctrl+C / SIGTERM)
```

Запуск как пользовательской службы systemd — см. комментарий в начале
`systemd/audio-censor.service`.

## Запуск на Linux: виртуальный микрофон

Чтобы зацензуренный звук можно было выбрать микрофоном в любом приложении (Discord, OBS,
диктофон, браузер), выход демона направляется в виртуальное устройство PipeWire. Код для этого
не меняется — только настройки системы.

```
микрофон ─► audio-censor ─► ALSA "censor" ─► censor_sink ══ loopback ══► censor_mic ─► Discord / OBS / ...
```

cpal на Linux работает через ALSA и видит только `default` и `PipeWire Sound Server`, а не
отдельные узлы PipeWire. Поэтому нужны две вещи: пара виртуальных узлов в PipeWire и
ALSA-устройство, через которое cpal до неё доберётся.

### 1. Виртуальный микрофон

`~/.config/pipewire/pipewire.conf.d/50-audio-censor.conf`:

```
context.modules = [
  { name = libpipewire-module-loopback
    args = {
      node.description = "Audio Censor"
      capture.props = {
        node.name        = "censor_sink"
        node.description = "Audio Censor (вход)"
        media.class      = "Audio/Sink"
        audio.position   = [ FL FR ]
      }
      playback.props = {
        node.name        = "censor_mic"
        node.description = "Censored Microphone"
        media.class      = "Audio/Source"
        audio.position   = [ FL FR ]
      }
    }
  }
]
```

Сторона `Audio/Sink` принимает звук демона, сторона `Audio/Source` видна приложениям как обычный
микрофон (monitor-устройства Discord и браузеры часто скрывают).

### 2. ALSA-устройства для демона

Узнать имя узла реального микрофона: `wpctl status` (id в разделе Sources), затем
`wpctl inspect <id> | grep node.name`.

`~/.asoundrc`:

```
# выход демона → виртуальный микрофон
pcm.censor {
    type pipewire
    playback_node "censor_sink"
    hint { show on  description "Audio Censor (виртуальный микрофон)" }
}

# вход демона — всегда реальный микрофон, а не «по умолчанию»
pcm.censor_in {
    type pipewire
    capture_node "alsa_input.<имя узла микрофона>"
    hint { show on  description "Audio Censor (реальный микрофон)" }
}
```

### 3. Применить и проверить

```sh
systemctl --user restart pipewire pipewire-pulse wireplumber
cargo run -- --show-device all     # должны появиться оба устройства «Audio Censor (...)»
```

### 4. Конфиг демона

```toml
[audio]
input_device  = "Audio Censor (реальный микрофон)"
output_device = "Audio Censor (виртуальный микрофон)"
sample_rate   = 48000
channels      = 2
sample_format = "F32"

[buffer]
delay_ms = 1500            # 5000 мс для живого разговора слишком много

[recognition]
partial_results = true     # иначе при малой задержке слова не успеют

[masking]
strategy = "Silence"       # шумоподавление Discord может вырезать бип
```

Имя устройства в конфиге — это `description` из `hint` в `~/.asoundrc`.

### 5. Приложения

- Микрофоном выбрать **Censored Microphone** (в Discord: Настройки → Голос и видео → Устройство
  ввода; шумоподавление Krisp лучше отключить).
- Слышать себя: `pw-loopback --capture-props='target.object=censor_mic'` — звук виртуального
  микрофона пойдёт на устройство вывода по умолчанию.
- Проверка: `pw-record --target censor_mic test.wav`, сказать слово из
  `config/banned_words.json`, прослушать запись.

### Подводные камни

- **Петля.** Не делать Censored Microphone микрофоном по умолчанию при `input_device = "default"`:
  демон начнёт слушать сам себя. `censor_in` закрепляет вход за реальным микрофоном.
- **Системный звук в микрофоне.** «Audio Censor (вход)» появится среди устройств вывода. Если
  сделать его выходом по умолчанию, весь звук системы уйдёт в микрофон.
- **Автозапуск.** `systemd/audio-censor.service` уже стартует после PipeWire
  (`After=pipewire.service … wireplumber.service`); свой конфиг — `-c` в `ExecStart`.

## Запуск на Windows

Создавать устройства Windows не умеет, поэтому выход демона направляется в виртуальный кабель
[VB-Audio Virtual Cable](https://vb-audio.com/Cable/): программа пишет в «CABLE Input», а другие
приложения (Discord, OBS, «Запись голоса», браузер) выбирают «CABLE Output» как микрофон.

```
микрофон ──► audio-censor ──► CABLE Input ══ кабель ══► CABLE Output ──► Discord / OBS / ...
```

### 1. Установка

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools   # затем в Visual Studio Installer:
                                                         # «Разработка классических приложений на C++»
winget install Rustlang.Rustup
winget install astral-sh.uv
winget install Git.Git
```

VB-Audio Virtual Cable: скачать с [vb-audio.com](https://vb-audio.com/Cable/), запустить
`VBCABLE_Setup_x64.exe` от администратора, перезагрузиться. После установок открыть новое окно
PowerShell, чтобы обновился `PATH`.

### 2. Сборка и модель

Проект лучше класть в путь без кириллицы и пробелов (например, `C:\dev\audio-censor`): Vosk
может не открыть модель по пути с не-ASCII символами.

```powershell
cd C:\dev
git clone <адрес репозитория> audio-censor
cd audio-censor
uv sync
cargo build --release
```

Модель `vosk-model-ru-0.22` распаковать в `models\vosk-model-ru-0.22\` (как в разделе выше).

### 3. Звук Windows

WASAPI в общем режиме принимает только собственное число каналов устройства и его частоту, а
`audio.channels` и `audio.sample_rate` общие для входа и выхода. Поэтому у микрофона и у
«CABLE Input» они должны совпадать:

1. `Win+R` → `mmsys.cpl`.
2. «Воспроизведение» → **CABLE Input** → Свойства → Дополнительно → «2 канала, 24 бит, 48000 Гц».
3. «Запись» → **микрофон** → Свойства → Дополнительно → вариант с 48000 Гц (и 2 каналами, если есть).
   У **CABLE Output** там же — 48000 Гц.
4. Проверить: `.\target\release\audio-censor.exe --show-device all` — у микрофона и CABLE Input
   одинаковые каналы, частота и формат (обычно `F32`).

Если микрофон только моно, а кабель стерео, попробуйте уменьшить число каналов кабеля в
`VBCABLE_ControlPanel.exe`. Если не получится, нужна доработка: разные `channels` для входа и
выхода.

«Параметры → Конфиденциальность и защита → Микрофон»: включить доступ классическим приложениям.

### 4. Конфиг

```powershell
copy config\default.toml config\config.toml
notepad config\config.toml
```

```toml
[audio]
input_device  = "Микрофон (Realtek(R) Audio)"   # точное имя микрофона, не "default"
output_device = "CABLE Input"
sample_rate   = 48000
channels      = 2          # как у обоих устройств в --show-device
sample_format = "F32"

[buffer]
delay_ms = 1500            # 5000 мс для живого разговора слишком много

[recognition]
partial_results = true     # иначе при малой задержке слова не успеют

[masking]
strategy = "Silence"       # шумоподавление Discord может вырезать бип
```

- Имя устройства — без того, что `--show-device` печатает после него: производителя в круглых
  скобках и типа в квадратных. Если демон пишет «устройство … не найдено», попробовать полное
  название из `mmsys.cpl` (например, `CABLE Input (VB-Audio Virtual Cable)`).
- `input_device = "default"` не оставлять: если Windows сделает CABLE Output микрофоном по
  умолчанию, демон начнёт слушать собственный выход.

### 5. Запуск

```powershell
.\target\release\audio-censor.exe -c config\config.toml
```

Лог — `logs\latest.log` (или `logs\latest.txt` с именем текущего лога, если Windows не разрешает
символические ссылки). Управление — из второго окна, через именованный канал:

```powershell
.\target\release\audio-censor.exe -c config\config.toml status
.\target\release\audio-censor.exe -c config\config.toml reload-dict
.\target\release\audio-censor.exe -c config\config.toml shutdown
```

Останавливать через `shutdown` или `Ctrl+C`, а не крестиком окна: при закрытии консоли у процесса
около 5 с, и хвост буфера с итоговыми метриками могут не сохраниться.

### 6. Приложения

- **Discord**: Настройки → Голос и видео → Устройство ввода «CABLE Output (VB-Audio Virtual Cable)»;
  отключить шумоподавление (Krisp), по желанию — эхоподавление и автоусиление.
- **OBS, «Запись голоса», браузер**: так же выбрать CABLE Output как микрофон.
- **Слышать себя**: `mmsys.cpl` → Запись → CABLE Output → Свойства → Прослушать →
  «Прослушивать с данного устройства».

Проверка: записать в «Записи голоса» с CABLE Output фразу со словом из
`config\banned_words.json` — оно должно быть заглушено.

### 7. Автозапуск

Не службой: службы работают в сеансе 0 и не видят аудиоустройств пользователя. Задача
Планировщика при входе в систему:

```powershell
schtasks /Create /TN "audio-censor" /SC ONLOGON /RL LIMITED `
  /TR "\"C:\dev\audio-censor\target\release\audio-censor.exe\" -c \"C:\dev\audio-censor\config\config.toml\""
```

### Неполадки

| Симптом | Причина |
|---|---|
| `link.exe not found` | не установлены C++ Build Tools |
| `build.rs` паникует про libvosk | не выполнен `uv sync` |
| «устройство … не найдено» | лишнее в имени устройства (п. 4) |
| «устройство не поддерживает формат …» | не совпали каналы или частота (п. 3) |
| ошибка загрузки модели | неверный `model_path` или кириллица в пути |
| в приложении тишина | выбран не CABLE Output, или `output_device` не CABLE Input |

## Конфигурация

`config/default.toml` — все параметры с комментариями; обязательны все поля. При запуске
`Settings::check()` проверяет значения (размер чанка, диапазоны громкости, приглушения,
частоты бипа, уверенности распознавания) и не даёт запуститься с ошибочными.

Словарь запрещённых слов — `config/banned_words.json`:

```json
{ "version": 1, "updated_at": "2026-08-06", "words": ["слово", "другое"] }
```

Слова сравниваются без учёта регистра и знаков препинания; для слов от 4 букв допускаются
опечатки (расстояние Левенштейна до `decision.max_distance`).

## Логи и метрики

Каждый запуск — отдельный сеанс с именем по времени запуска (UTC):

```
logs/
├── 2026-10-18_13-37-54.log            # лог сеанса
├── 2026-10-18_13-37-54.metrics.json   # метрики сеанса с параметрами конфига
├── latest.log -> 2026-10-18_13-37-54.log
└── sessions.jsonl                     # строка на каждый завершённый сеанс — для сравнения опытов
```

Каталоги задаются в `logging.dir` и `metrics.dir`. В файле метрик `"final": false` —
промежуточный снимок по команде `save-metrics`, `true` — итог при остановке.

## Структура

```
audio-censor/
├── build.rs                     # поиск libvosk (.venv / lib/), линковка, копирование в target/, rpath
├── config/
│   ├── default.toml             # конфигурация по умолчанию
│   └── banned_words.json        # словарь запрещённых слов
├── systemd/
│   └── audio-censor.service     # пользовательская служба systemd
├── doc/                         # диплом (Typst), описывающий программу
├── src/
│   ├── main.rs                  # аргументы, режимы (демон / клиент управления / список устройств)
│   ├── lib.rs                   # перевод мс → кадры / позиции в потоке, whole_frames
│   ├── logging.rs               # инициализация логирования
│   ├── config/
│   │   ├── settings.rs          # структуры конфигурации, загрузка и проверка
│   │   └── ban_set.rs           # загрузка словаря
│   ├── capture/                 # устройство ввода и колбэк захвата (cpal)
│   ├── playback/                # устройство вывода и колбэк воспроизведения (cpal)
│   ├── buffer/
│   │   ├── ring.rs              # SPSC-кольца между колбэками и оркестратором
│   │   └── delay_buffer.rs      # задержка звука с абсолютными позициями сэмплов
│   ├── framing/
│   │   ├── convert.rs           # перевод в моно i16 для распознавания
│   │   └── chunker.rs           # нарезка на чанки встык
│   ├── recognition/
│   │   ├── recognizer.rs        # обёртка над Vosk, слова с таймстемпами и уверенностью
│   │   └── events.rs            # TimedWord
│   ├── decision/
│   │   ├── rules.rs             # сравнение слова со словарём
│   │   └── engine.rs            # слова → интервалы для маскирования
│   ├── masking/
│   │   ├── masker.rs            # наложение маски по позициям, фейды, слияние интервалов
│   │   └── strategies.rs        # тишина, бип, приглушение
│   ├── pipeline/
│   │   ├── orchestrator.rs      # основной цикл: вход → задержка → маска → выход
│   │   ├── recognition_worker.rs # поток распознавания: чанки → Vosk → решения
│   │   └── drift.rs             # компенсация дрейфа часов между устройствами
│   ├── control/
│   │   ├── ipc.rs               # сокет управления (Unix-сокет / именованный канал Windows)
│   │   └── commands.rs          # status, reload-dict, save-metrics, shutdown
│   └── metrics/
│       └── stats.rs             # счётчики потерь, распознанных и заглушённых слов, опозданий
└── tests/                       # тесты без аудиоустройств и без Vosk
```

## Как это работает

Четыре потока:

1. **Колбэк захвата** кладёт сэмплы во входное кольцо.
2. **Оркестратор** забирает их и ведёт две ветки:
   - звук → `DelayBuffer` (задержка `delay_ms`) → `Masker` → компенсация дрейфа → выходное кольцо;
   - копия в моно i16 → поток распознавания.
3. **Поток распознавания**: чанки → Vosk → `DecisionEngine` → интервалы запрещённых слов
   обратно оркестратору, который регистрирует их в `Masker`.
4. **Колбэк воспроизведения** забирает звук из выходного кольца.

Маска срабатывает, только если интервал зарегистрирован раньше, чем звук слова выйдет
из задержки. Если нет — растёт метрика `late_intervals`: это главный показатель того,
хватает ли `delay_ms`.
