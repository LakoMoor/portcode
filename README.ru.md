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

Промпты и скилл оптимизированы под открытые модели (DeepSeek, Qwen, в т. ч. «flash»/малые): короткие нумерованные правила, пример в каждом инструменте, фазы с «Done when», починка битого JSON и tool-call-ов, написанных текстом, стабильный префикс для кэша llama.cpp/vLLM. **Это не результаты бенчмарков**: запусти `portcode eval` на своей модели и подстрой профиль (`docs/PROVIDERS.md`).

Пример порта с реальными данными: [examples/wordstat](examples/wordstat) · сгенерированный сайт: `docs/ports/wordstat/`.
Статус: альфа. Изучай только ПО, которое тебе принадлежит или на анализ которого есть разрешение.
