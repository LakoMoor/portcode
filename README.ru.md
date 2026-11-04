# PortCode (по-русски)

**Портируй что угодно на Rust — и покажи доказательства.** Терминальный агент для реверс-инжиниринга и портов на Rust, который работает с *твоими* моделями: `lakomoor.com/v1`, Ollama, LM Studio, llama.cpp, vLLM.

Что делает: фиксирует точную сборку оригинала (sha256), ведёт журнал доказательств (`observation` — видел, `inference` — вывел, `unknown` — пробел), портит модуль за модулем против golden-выводов **оригинала**, проверяет `parity` и дифференциальным фаззингом, делает скриншот на каждую функцию и генерирует сайт для GitHub Pages.

Это переписывание архитектуры OpenCode на Rust (агенты, инструменты, разрешения, сессии, скиллы, MCP) вместе со скиллом [rustport](skills/rustport). Что покрыто, а что нет — в [docs/OPENCODE_MAPPING.md](docs/OPENCODE_MAPPING.md).

![Интерфейс PortCode](docs/screenshots/tui-session.svg)

Установка (macOS / Linux / WSL; проверяет sha256):
```sh
curl -fsSL https://raw.githubusercontent.com/LakoMoor/portcode/main/install.sh | sh
# или: cargo install --git https://github.com/LakoMoor/portcode portcode-cli
```
Полноэкранный интерфейс в стиле OpenCode: `enter` отправить · `alt+enter` перенос · `tab` сменить агента · `/` команды · `esc` прервать · `ctrl+t` рассуждения · `ctrl+b` боковая панель. `portcode --plain` — построчный режим. Windows: через WSL.

```sh
portcode init && export LAKOMOOR_API_KEY=...   # или portcode auth set lakomoor
portcode doctor
portcode -a port                               # воркфлоу rustport
portcode -m ollama/qwen3:14b                   # локальная модель, ключ не нужен
```

**Доводит до конца 1:1.** Агент `port` не может остановиться раньше времени: пока не все поведения оригинала в todo, не все пункты выполнены или последний `parity` не прошёл на финальном коде, его возвращают в работу и при необходимости добавляют шагов. Выйти можно только через `blocked` — когда нужно решение от тебя.

**Remote control.** `portcode serve` (или `/remote` в интерфейсе) — управление сессией из браузера или с телефона на [lakomoor.github.io/portcode/remote](https://lakomoor.github.io/portcode/remote/): смотреть ход работы, слать промпты, отвечать на запросы разрешений, прерывать, менять агента и режим. GitHub Pages хранит только статическую страницу; браузер подключается **напрямую** к твоей машине через туннель (Tailscale, cloudflared, SSH) по ключу `portcode remote new`. Секрет по сети не передаётся (HMAC-ответ на случайный вызов), чужого relay нет. Подробности: [docs/REMOTE.md](docs/REMOTE.md).

**Режимы.** `clean-room` (по умолчанию) — оригинал изучается только снаружи. `--deep` / `/deep` — можно смотреть внутрь: инструменты `disasm` (Capstone), `emulate` (Unicorn), `decompile` (Ghidra headless), `hexdump`, `functions`; в интерфейсе `/workbench` показывает функции, Pseudo-C, дизассемблер, hex и журнал доказательств рядом (клавиши: `j/k`, `1 2 3`, `/` фильтр, `a` — спросить агента); ImHex, Keystone, LIEF, YARA, angr, Frida и rizin/radare2/binwalk/capa/jadx подключаются тоже. Движки ставятся (с вопросом и проверкой sha256) в папку `engines/` рядом с бинарём: `portcode engines install core`. Самая строгая схема: сессия `reverse --deep` пишет `docs/SPEC.md` своими словами, сессия `port --clean-room` строит порт только по ней. Подробнее: [docs/ENGINES.md](docs/ENGINES.md).

Промпты и скилл оптимизированы под открытые модели (DeepSeek, Qwen, в т. ч. «flash»/малые): короткие нумерованные правила, пример в каждом инструменте, фазы с «Done when», починка битого JSON и tool-call-ов, написанных текстом, стабильный префикс для кэша llama.cpp/vLLM. **Это не результаты бенчмарков**: запусти `portcode eval` на своей модели и подстрой профиль (`docs/PROVIDERS.md`).

Пример порта с реальными данными: [examples/wordstat](examples/wordstat) · сгенерированный сайт: `docs/ports/wordstat/`.
Статус: альфа. Изучай только ПО, которое тебе принадлежит или на анализ которого есть разрешение.
