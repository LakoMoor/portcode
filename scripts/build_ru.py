#!/usr/bin/env python3
"""Build docs/ru/index.html from docs/index.html: same page, Russian text. Every English string below must exist in
the source (the script fails otherwise), so the two pages cannot drift apart silently."""
import re, sys, pathlib

root = pathlib.Path(__file__).resolve().parent.parent
src = (root / "docs/index.html").read_text()

T = [
 ('<small class="ik" style="margin-top:20px">Then open any folder</small>', '<small class="ik" style="margin-top:20px">Потом откройте любую папку</small>'),
 ('<small class="ik">Install in one line</small>', '<small class="ik">Установка одной строкой</small>'),
 ('<small class="ik" style="margin-top:20px">Windows (PowerShell)</small>', '<small class="ik" style="margin-top:20px">Windows (PowerShell)</small>'),
 ('<p class="ikn">macOS, Linux, Windows and WSL. The installer checks the sha256 of what it downloads. Also runs on your <a href="wiki/subscriptions.html">Claude Code or Codex subscription</a>, no API key.</p>', '<p class="ikn">macOS, Linux, Windows и WSL. Установщик проверяет sha256 загрузки. Работает и на вашей <a href="wiki/subscriptions.html">подписке Claude Code или Codex</a>, без API-ключа.</p>'),
 ('Use your Claude or ChatGPT plan', 'Работай на своей подписке Claude или ChatGPT'),
 ('No API key needed. PortCode starts the official <code>claude</code> or <code>codex</code> program on your machine and lets it use the login you already have. It adds its own tools: the evidence ledger, pinning, parity checks and screenshots.', 'Ключ API не нужен. PortCode запускает на твоём компьютере официальную программу <code>claude</code> или <code>codex</code>, и она использует твой вход. Свои инструменты PortCode добавляет сам: журнал доказательств, фиксацию оригинала, проверки parity и скриншоты.'),
 ('PortCode on your plan', 'PortCode на твоей подписке'),
 ('<code>portcode -m claude-code/sonnet</code> or <code>portcode -m codex/default</code>. The work shows up in the PortCode screen and the evidence goes into the same ledger.', '<code>portcode -m claude-code/sonnet</code> или <code>portcode -m codex/default</code>. Работа видна в окне PortCode, а доказательства попадают в тот же журнал.'),
 ('Your CLI uses PortCode', 'Твой CLI использует PortCode'),
 ('<code>claude mcp add portcode -- portcode mcp</code> gives Claude Code (and Codex with <code>codex mcp add</code>) PortCode\'s tools in the session you already use. <a href="wiki/subscriptions.html">Details and limits</a>.', '<code>claude mcp add portcode -- portcode mcp</code> даёт Claude Code (а Codex через <code>codex mcp add</code>) инструменты PortCode в привычной сессии. <a href="wiki/subscriptions.html">Подробности и ограничения</a>.'),
 ('>First run</a>', '>Первый запуск</a>'),
 ('<h2>First run</h2>', '<h2>Первый запуск</h2>'),
 ('Three steps, all inside the app. Type <code>/setup</code> any time to see them again.', 'Три шага, все внутри приложения. <code>/setup</code> покажет их снова.'),
 ('1. Pick a provider', '1. Выбери провайдера'),
 ('Local model, no key: <code>/provider ollama qwen3:14b</code>. Type <code>/provider</code> alone to see all of them and which need a key.', 'Локальная модель, ключ не нужен: <code>/provider ollama qwen3:14b</code>. Просто <code>/provider</code> покажет все и у каких нет ключа.'),
 ('2. Paste your key', '2. Вставь ключ'),
 ('<code>/key</code> asks for it. It is hidden while you type and saved with mode 600. Or run <code>portcode auth set &lt;provider&gt;</code> in a shell.', '<code>/key</code> спросит его. Пока вводишь, он скрыт; сохраняется с правами 600. Или в терминале: <code>portcode auth set &lt;provider&gt;</code>.'),
 ('3. Check and start', '3. Проверь и начни'),
 ('<code>portcode doctor</code> tests the setup. Then <code>/create</code>, or just type a task. <a href="wiki/first-run.html">Full guide</a>.', '<code>portcode doctor</code> проверит настройку. Потом <code>/create</code> или просто напиши задачу. <a href="wiki/first-run.html">Подробная инструкция</a>.'),
 ('Updates: <code>portcode update</code> or <code>/update</code> compares your version with the latest release. Once a day the app checks quietly. <code>PORTCODE_NO_UPDATE_CHECK=1</code> turns it off. Nothing installs by itself.', 'Обновления: <code>portcode update</code> или <code>/update</code> сравнивает твою версию с последним релизом. Раз в день приложение проверяет тихо. <code>PORTCODE_NO_UPDATE_CHECK=1</code> отключает. Сами обновления не ставятся.'),
 ('Watch a real run', 'Посмотри настоящий запуск'),
 ("This is the wordstat port. The tool calls and their output were recorded from a real run, and the model's replies are scripted so it is repeatable. Nothing here is typed by hand.", 'Это порт wordstat. Вызовы инструментов и их вывод записаны с настоящего запуска, а ответы модели заранее написаны, чтобы запуск повторялся. Ничего не напечатано вручную.'),
 ('data-lang="en"', 'data-lang="ru"'),
 ('<h4>Evidence</h4>', '<h4>Доказательства</h4>'),
 ('<h4>To do</h4>', '<h4>Дела</h4>'),
 ('>Play</button>', '>Пуск</button>'),
 ('Try /create', 'Попробуй /create'),
 ('What do you want to do?', 'Что хочешь сделать?'),
 ('>Recreate it<', '>Воссоздать<'),
 ('>Just look inside<', '>Только изучить<'),
 ('What is it?', 'Что это?'),
 ('>Website<', '>Сайт<'),
 ('>Program<', '>Программа<'),
 ('>Game<', '>Игра<'),
 ('>File format<', '>Формат файла<'),
 ('How closely may it look?', 'Насколько близко смотреть?'),
 ('>Outside only<', '>Только снаружи<'),
 ('>Inside too<', '>И внутри<'),
 ('Build with', 'На чём строить'),
 ('<html lang="en">', '<html lang="ru">'),
 ('<title>PortCode: port a program to Rust and check it behaves the same</title>', '<title>PortCode: перенеси программу на Rust и проверь, что она ведёт себя так же</title>'),
 ('content="PortCode: port a program to Rust and check it behaves the same"', 'content="PortCode: перенеси программу на Rust и проверь, что она ведёт себя так же"'),
 ('Open-source terminal agent that reverse-engineers programs and ports them to Rust. Works with local and self-hosted models.', 'Открытый терминальный агент: изучает программу и переписывает её на Rust. Работает с локальными и своими моделями.'),
 ('PortCode is an open-source terminal agent. It studies a program, rewrites it in Rust, and checks the result against the original with recorded evidence, golden tests and screenshots. It works with local and self-hosted models: Ollama, LM Studio, llama.cpp, vLLM.', 'PortCode — открытый терминальный агент. Он изучает программу, переписывает её на Rust и сверяет результат с оригиналом: записанные факты, эталонные тесты, скриншоты. Работает с локальными и своими моделями: Ollama, LM Studio, llama.cpp, vLLM.'),
 # nav
 ('>Deep mode</a>', '>Режим deep</a>'), ('>How it works</a>', '>Как это работает</a>'), ('>Local LLMs</a>', '>Модели</a>'),
 ('>Ports</a>', '>Порты</a>'), ('>Remote</a>', '>Пульт</a>'), ('>Docs</a>', '>Документация</a>'), ('>RU</a>', '>EN</a>'),
 # hero
 ('Port a program to Rust and check it behaves the same.', 'Перенеси программу на Rust и проверь, что она ведёт себя так же.'),
 ("PortCode is a terminal agent. It studies the original, writes down what it finds out, rewrites the program in Rust piece by piece, and compares each piece with the original's real output. It works with models you run yourself:", 'PortCode — терминальный агент. Он изучает оригинал, записывает, что удалось выяснить, переписывает программу на Rust по кусочку и сверяет каждый кусок с настоящим выводом оригинала. Работает с моделями, которые ты запускаешь сам:'),
 ('>Install</a>', '>Установить</a>'), ('See an example port', 'Пример порта'),
 ('>Clean-room</span>', '>Clean-room</span>'),
 ('150 steps', '150 шагов'), ('Pin the build', 'Зафиксировать сборку'), ('Golden cases', 'Эталонные случаи'), ('Parity PASS', 'Parity PASS'),
 ('>Screenshots</div>', '>Скриншоты</div>'), ('Run parity', 'Запустить parity'), ('>context<', '>контекст<'),
 ('● connected · full control', '● подключено · полное управление'), ('Port wordstat to Rust', 'Перенеси wordstat на Rust'),
 ('bash asks to run', 'bash просит запустить'), ('>Allow<', '>Разрешить<'), ('>Deny<', '>Запретить<'),
 ('<small>agent</small>', '<small>агент</small>'), ('<small>stop</small>', '<small>стоп</small>'), ('<small>menu</small>', '<small>меню</small>'),
 ('<small>allow</small>', '<small>да</small>'), ('<small>deny</small>', '<small>нет</small>'), ('<small>send</small>', '<small>ввод</small>'),
 ('wordstat · C → Rust', 'wordstat · C → Rust'), ('1 unknown', '1 неизвестное'), ('Open the port', 'Открыть порт'), ('>Ledger<', '>Журнал<'),
 ('The controls and the remote page are copies of the real interface. The numbers come from the wordstat example in the repo. In the screenshots the model is a scripted replay, so anyone can reproduce them.', 'Элементы управления и страница пульта — копии настоящего интерфейса. Цифры взяты из примера wordstat в репозитории. В скриншотах модель — записанный сценарий, поэтому всё воспроизводимо.'),
 # create
 ('Start with /create', 'Начните с /create'),
 ('Type <code>/create</code> and answer a few questions: what to recreate (a website, a program, a game, a file format, or nothing yet), how closely PortCode may look at it, what to build with, and what you want. It starts the right agent in the right mode. Pick "Just look inside" if you only want to understand it: you get <code>docs/SPEC.md</code> and the evidence ledger, no port code.', 'Введите <code>/create</code> и ответь на несколько вопросов: что воссоздать (сайт, программу, игру, формат файла или пока ничего), насколько близко PortCode может в это заглядывать, на чём строить и чего ты хочешь. Он запустит нужного агента в нужном режиме. Выберите «Just look inside», если нужно только разобраться: получите <code>docs/SPEC.md</code> и журнал фактов, без кода порта.'),
 ('The /create screen set up for a website', 'Экран /create для сайта'), ('The /create screen set to just look inside a program', 'Экран /create в режиме «только посмотреть» для программы'),
 # tui
 ('The terminal interface', 'Терминальный интерфейс'),
 ('The port agent working on wordstat: tool calls, evidence and a to-do list on the right. Made with', 'Агент port работает над wordstat: вызовы инструментов, факты и список дел справа. Снято скриптом'),
 # install
 ('<h2>Install</h2>', '<h2>Установка</h2>'),
 ('Windows, Linux and macOS, x86_64 and aarch64. The installers check the sha256; the Linux/macOS one builds from source if there is no binary for your system. The Windows build needs no Rust and no Visual C++ runtime.', 'Windows, Linux и macOS, x86_64 и aarch64. Установщики проверяют sha256; установщик для Linux/macOS собирает из исходников, если бинарника для твоей системы нет. Сборке для Windows не нужны ни Rust, ни Visual C++.'),
 # deep
 ('<h2>Clean room or deep</h2>', '<h2>Clean room или deep</h2>'),
 ('In clean-room mode PortCode only watches the original from outside: it runs it and compares the output. It will not use disassemblers, decompilers or hex viewers. In deep mode it uses Capstone, Unicorn, Ghidra and ImHex, which are installed next to the binary, each one only after you say yes.', 'В режиме clean-room PortCode смотрит на оригинал только снаружи: запускает и сравнивает вывод. Дизассемблеры, декомпиляторы и hex-просмотрщики не используются. В режиме deep он работает с Capstone, Unicorn, Ghidra и ImHex. Они ставятся рядом с бинарником, каждый — только после твоего «да».'),
 ('Ghidra reads the whole function.', 'Ghidra разбирает функцию целиком.'),
 ('The default value is recorded as an inference until output from the original confirms it.', 'Значение по умолчанию записано как inference, пока вывод оригинала его не подтвердит.'),
 ('shows the first instructions: the function reserves 416 bytes of stack.', 'показывает первые инструкции: функция резервирует 416 байт стека.'),
 ('runs it.', 'запускает.'), ('sp before', 'sp до'), ('sp after', 'sp после'), ('8 instructions · frame 0x1a0 bytes', '8 инструкций · кадр 0x1a0 байт'),
 ('PortCode works with', 'PortCode работает с'), ('tools. Each one is installed only after you say yes.', 'инструментами. Каждый ставится только после твоего «да».'),
 ('describes a file format. <span>The agent writes the pattern, you check it in ImHex.</span>', 'описывает формат файла. <span>Агент пишет шаблон, ты проверяешь его в ImHex.</span>'),
 ('Around them', 'Рядом'), ('rizin, radare2, YARA, Frida, LIEF, capa and Detect It Easy, plus ILSpy, Il2CppDumper, dnfile and UnityPy for Unity and C#.', 'rizin, radare2, YARA, Frida, LIEF, capa и Detect It Easy, а для Unity и C# ещё ILSpy, Il2CppDumper, dnfile и UnityPy.'),
 ('Nothing looks inside unless you turn it on.', 'Внутрь никто не заглядывает, пока ты не включишь.'),
 ('Clean room is the default. Switch to deep in the terminal or on the remote page.', 'По умолчанию — clean room. Deep включается в терминале или на странице пульта.'),
 ('The numbers on these cards come from running the tools on the wordstat binary from the repo.', 'Числа на этих карточках получены запуском инструментов на бинарнике wordstat из репозитория.'),
 ('in deep mode. Functions on the left; pseudo-C, disassembly or hex in the middle; the evidence ledger on the right; the agent at the bottom.', 'в режиме deep. Слева функции, в центре псевдо-C, дизассемблер или hex, справа журнал фактов, внизу агент.'),
 ('makes the agent explain the function and record what it finds.', 'заставляет агента объяснить функцию и записать, что он нашёл.'),
 ('<p>Type <code', '<p>Введите <code'),
 ('>remote</a>', '>пульт</a>'),
 ('Two sessions, kept apart', 'Две сессии, разделённые стеной'),
 ('The first session looks inside and writes', 'Первая сессия смотрит внутрь и пишет'),
 (', the second builds the port from that spec alone, in clean-room mode.', ', вторая строит порт только по этой спецификации, в режиме clean-room.'),
 # how
 ('<h2>Facts first, then code</h2>', '<h2>Сначала факты, потом код</h2>'),
 ('>Study<', '>Изучение<'), ('>Record<', '>Запись<'), ('>Port<', '>Порт<'), ('>Prove<', '>Проверка<'),
 ('Fix the exact build, look through the files, follow the main loop. What you only read is an <code>inference</code>; only what you ran counts as an <code>observation</code>.', 'Фиксируем точную сборку, просматриваем файлы, идём по главному циклу. Что только прочитано — <code>inference</code>; <code>observation</code> — только то, что запускали.'),
 ('A log you can only add to. Each entry has an id (<code>E-0007</code>), a source and a hash, and open questions are marked <code>unknown</code>. Every rule in the port points to an entry.', 'Журнал, в который можно только дописывать. У каждой записи есть id (<code>E-0007</code>), источник и хеш, открытые вопросы помечены <code>unknown</code>. Каждое правило в порте ссылается на запись.'),
 ('One small piece at a time, checked against output captured from the original, not against your reading of its code.', 'По одному небольшому куску, сверяя с выводом оригинала, а не с твоим прочтением его кода.'),
 ('and fuzzing against the original decide whether the port matches. The agent will not stop before a PASS.', 'и фаззинг против оригинала решают, совпадает ли порт. Агент не остановится до PASS.'),
 # llm
 ('<h2>Works with local and self-hosted models</h2>', '<h2>Работает с локальными и своими моделями</h2>'),
 ('One OpenAI-compatible client that copes with what open models get wrong: <code>reasoning_content</code> and inline <code>&lt;think&gt;</code>, tool calls written as plain text, broken or cut-off JSON arguments, numbers in quotes, servers that reject optional fields, streams that stall. Small context windows and replies cut off by the output limit are handled too.', 'Один OpenAI-совместимый клиент, который переносит то, в чём ошибаются открытые модели: <code>reasoning_content</code> и <code>&lt;think&gt;</code> в тексте, вызовы инструментов обычным текстом, битый или оборванный JSON в аргументах, числа в кавычках, серверы, отвергающие необязательные поля, зависающие потоки. Маленькое окно контекста и ответы, обрезанные лимитом вывода, тоже учтены.'),
 ('>Provider<', '>Провайдер<'), ('>Default endpoint<', '>Адрес по умолчанию<'), ('>Key<', '>Ключ<'), ('>none<', '>не нужен<'), ('>optional<', '>по желанию<'),
 ('hosted, same protocol', 'облако, тот же протокол'), ('env var', 'переменная окружения'),
 ('<strong>What is tested.</strong> The client, the JSON repair and the agent loop are tested end to end against a scripted server. The DeepSeek and Qwen profiles are cautious guesses, not benchmark results. Run <code>portcode eval</code> on your model and adjust.', '<strong>Что проверено.</strong> Клиент, починка JSON и цикл агента протестированы целиком на сценарном сервере. Профили DeepSeek и Qwen — осторожные догадки, а не результаты бенчмарков. Запустите <code>portcode eval</code> на своей модели и подправьте.'),
 # agents
 ('<h2>Six agents</h2>', '<h2>Шесть агентов</h2>'),
 ('Default. Makes changes from start to finish.', 'По умолчанию. Доводит изменения от начала до конца.'),
 ('The rustport workflow, the porting tools and the finish check.', 'Процесс rustport, инструменты порта и проверка завершения.'),
 ('Studies the original and records evidence. Does not edit code. Uses the deep tools.', 'Изучает оригинал и записывает факты. Код не правит. Использует инструменты deep.'),
 ('Read-only: investigates and writes a plan.', 'Только чтение: исследует и пишет план.'),
 ('Sub-agents with a clean context, started through', 'Подагенты с чистым контекстом, запускаются через'),
 ('Edits and commands ask first (allow, always, deny). Dangerous commands are always refused and file tools stay inside the project. The shell runs with stdin closed, a time limit and capped output, and the whole process group is killed on timeout.', 'Правки и команды сначала спрашивают (разрешить, всегда, запретить). Опасные команды отклоняются всегда, файловые инструменты не выходят за проект. Оболочка работает с закрытым stdin, лимитом времени и ограниченным выводом; при таймауте убивается вся группа процессов.'),
 # skill
 ('<h2>The rustport skill</h2>', '<h2>Навык rustport</h2>'),
 ('It is built into the binary and also works in Claude Code, Codex and OpenCode (<code>portcode skill export ~/.claude/skills</code>). The main file is about 100 lines and loads the detailed notes only when a phase needs them.', 'Он встроен в бинарник и работает также в Claude Code, Codex и OpenCode (<code>portcode skill export ~/.claude/skills</code>). Основной файл — около 100 строк, подробные заметки подгружаются, только когда нужны этапу.'),
 ('<b>Intake</b> <span>fidelity, platforms, clean-room or deep, licence →', '<b>Заявка</b> <span>точность, платформы, clean-room или deep, лицензия →'),
 ('<b>Pin + survey</b> <span>exact build hash, inventory, unknowns recorded</span>', '<b>Фиксация и обзор</b> <span>хеш сборки, опись, неизвестные записаны</span>'),
 ('<b>Plan</b> <span>module map leaves-first; rewrite, bind or drop per module</span>', '<b>План</b> <span>карта модулей от листьев; на каждый — переписать, подключить или отбросить</span>'),
 ('<b>Golden harness</b> <span>deterministic scenarios captured from the original</span>', '<b>Эталоны</b> <span>детерминированные сценарии, снятые с оригинала</span>'),
 ('<b>Port core</b> <span>slice → test → clippy → parity; bisect, never widen tolerance</span>', '<b>Ядро порта</b> <span>кусок → тест → clippy → parity; искать расхождение бисекцией, допуск не расширять</span>'),
 ('<b>Platform / rendering</b> <span>only for a GUI; wgpu behind traits, offscreen PNG compare</span>', '<b>Платформа и графика</b> <span>только для GUI; wgpu за трейтами, сравнение PNG вне экрана</span>'),
 ('<b>Prove it</b> <span>screenshots per feature, perf,', '<b>Доказательство</b> <span>скриншот на каждую функцию, скорость,'),
 # ports
 ('<h2>An example port</h2>', '<h2>Пример порта</h2>'),
 ('wordstat, C → Rust', 'wordstat, C → Rust'),
 ('A real C program, 6 golden cases, 400 fuzz runs against the original, 11 evidence records and one open question.', 'Настоящая программа на C, 6 эталонных случаев, 400 прогонов фаззинга против оригинала, 11 записей фактов и один открытый вопрос.'),
 ('Open the generated site', 'Открыть сгенерированный сайт'), ('The ledger, in the terminal', 'Журнал в терминале'),
 ('lists what is observed, inferred and unknown for that port.', 'показывает, что для этого порта наблюдалось, предполагалось и неизвестно.'),
 # cli
 ('<h2>Everything is a tool</h2>', '<h2>Всё — инструмент</h2>'),
 ('The terminal UI, the agent and scripts call the same tools. Anything the model can do, you can run by hand with', 'Терминальный интерфейс, агент и скрипты вызывают одни и те же инструменты. Всё, что умеет модель, можно запустить руками:'),
 ('>Command<', '>Команда<'), ('>What it does<', '>Что делает<'),
 ('Interactive chat', 'Диалог'), ('One-shot, JSON lines', 'Один запрос, JSON-строки'), ('Check setup · probe a model', 'Проверка настроек · проверка модели'),
 ('The porting workbench, no LLM', 'Инструменты порта, без LLM'), ('Remote control, see', 'Удалённое управление, см.'), ('Install the skill elsewhere', 'Поставить навык в другой агент'),
 # lineage
 ('<h2>Where this comes from</h2>', '<h2>Откуда это</h2>'),
 ('PortCode follows the architecture of <a href="https://github.com/anomalyco/opencode">OpenCode</a>, rewritten in Rust, and combines it with the <a href="https://github.com/LakoMoor/rustport-skill">rustport</a> skill and ideas from <a href="https://github.com/storytold/craft-anything">Craft Anything</a>. It is a rewrite, not a fork. <a href="https://github.com/LakoMoor/portcode/blob/main/docs/OPENCODE_MAPPING.md">The mapping</a> lists what is covered.', 'PortCode повторяет архитектуру <a href="https://github.com/anomalyco/opencode">OpenCode</a>, переписанную на Rust, и объединяет её с навыком <a href="https://github.com/LakoMoor/rustport-skill">rustport</a> и идеями <a href="https://github.com/storytold/craft-anything">Craft Anything</a>. Это переписывание, а не форк. <a href="https://github.com/LakoMoor/portcode/blob/main/docs/OPENCODE_MAPPING.md">Таблица соответствия</a> показывает, что покрыто.'),
 ('MIT licensed · OpenCode is © its authors (MIT), see NOTICE · Screenshots are captured from real runs and never edited.', 'Лицензия MIT · OpenCode © его авторы (MIT), см. NOTICE · Скриншоты сняты с настоящих запусков и не редактируются.'),
]
out = src
missing = [a for a, _ in T if a not in out]
if missing:
    print("missing in docs/index.html:\n  " + "\n  ".join(m[:90] for m in missing)); sys.exit(1)
for a, b in T:
    out = out.replace(a, b)
# the language switch and local paths
out = out.replace('href="ru/"', 'href="../"')
for d in ("style.css", "toolkit.css", "theme.js", "copy.js", "demo.js", "demo/", "assets/", "screenshots/", "ports/", "guide/", "wiki/", "remote/", "media/"):
    out = out.replace(f'href="{d}', f'href="../{d}').replace(f'src="{d}', f'src="../{d}')
out = out.replace('<a class="l" href="../wiki/">Документация</a>', '<a class="l" href="../wiki/">Документация</a>')
out = out.replace('href="#"', 'href="./"')
(root / "docs/ru").mkdir(exist_ok=True)
(root / "docs/ru/index.html").write_text(out)
# what is still English in visible text (code and identifiers are fine); printed for review
body = re.sub(r"<(pre|script|style|svg)\b.*?</\1>", "", out, flags=re.S)
left = sorted({t.strip() for t in re.findall(r">([^<>]{6,})<", body) if re.search(r"[A-Za-z]{4,} [a-z]{3,} [a-z]{2,}", t) and not re.search(r"[а-яё]", t, re.I)})
print("wrote docs/ru/index.html;", len(left), "English-looking text nodes left")
for t in left[:40]:
    print("  ", t[:100])
