# 3. job.yaml — декларативный формат заданий (веха M3)

Запуск:

```bash
arachne crawl --job job.yaml --output out.jsonl [--delay-ms 300] [--task-id 7]
```

- `--output` — формат вывода определяется расширением: `.csv` / `.jsonl` / `.json` / `.sqlite`/`.db`;
- `--delay-ms` — средняя задержка между запросами (база гауссова джиттера, по умолчанию 200);
- `--task-id` — переопределить id задачи в записях (по умолчанию 1).

## Структура

```yaml
# Стартовые URL (поддерживают {var} из loops)
start_urls:
  - https://quotes.toscrape.com/page/{page}/

# Циклы подстановки переменных
loops:
  - var: page
    range: { start: 1, end: 5 }

# Плоские селекторы
selectors:
  - name: page_title
    selector: h1

# Вложенные селекторы (списки с полями)
nested_selectors:
  - repeat_selector: .quote
    fields:
      - name: quote_text
        selector: .text
      - name: quote_author
        selector: .author
    nested:                          # вложенный уровень (рекурсия)
      - repeat_selector: .tags .tag
        fields:
          - name: tag_name
            selector: "."

# Переход по ссылкам (BFS-обход)
follow:
  selector: "ul.pager a"
  max_depth: 3

# Заголовки и cookie запросов (поддерживают {var})
headers:
  - name: Accept-Language
    value: en-US
cookies:
  - name: session
    value: "{page}"

# Лимит страниц
limit: 100
```

## Поля

| Поле | Описание | Статус |
|---|---|---|
| `start_urls` | Список URL для начала краулинга; поддерживают плейсхолдеры `{var}` | ✓ |
| `selectors` | Плоские CSS-селекторы: `name` + `selector`. Извлекает первое совпадение | ✓ |
| `nested_selectors` | Вложенные селекторы для списков (см. ниже) | ✓ |
| `limit` | Опциональный лимит страниц (считает **все** страницы, включая найденные по `follow`) | ✓ |
| `follow` | Переход по ссылкам (BFS-обход) — см. §5 | ✓ |
| `loops` | Циклы подстановки `{var}` в URL/headers/cookies — см. §4.2 | ✓ |
| `while` | Цикл с авто-границей остановки — см. §4.3 | ✓ |
| `output_template` | Шаблон JSON-структуры вывода (для `--output *.json`) | ✓ |
| `headers` | Дополнительные заголовки запроса (`name` + `value`, поддерживают `{var}`) | ✓ |
| `cookies` | Cookie запросов (`name` + `value`, поддерживают `{var}`) | ✓ |
| `proxies` | Пул прокси (round-robin) | ✓ |

## Вложенные селекторы (§4)

Вложенный поиск позволяет извлекать структурированные данные из повторяющихся
блоков на странице. Алгоритм:

1. **Сначала** ищется родительский блок по `repeat_selector` (например, `.quote`)
2. **Внутри каждого** совпадения ищутся поля по их CSS-селекторам
3. Каждая запись получает `index` — номер блока (0-based)

### Рекурсивная вложенность (§4.1)

Внутри блока можно объявить секцию `nested` — вложенные repeat-селекторы,
которые ищутся **внутри каждого найденного блока**. Глубина вложенности —
произвольная (рекурсия).

Путь индексов в имени поля кодирует иерархию: `name[i]` на корневом
уровне, `name[i.j]` — вложенный блок `j` внутри блока `i`.

**Спец-селектор `"."`** означает «текст самого блока» — полезен для
листовых элементов (например, `a.tag`), у которых нет дочерних полей.

### Пример: цитаты с тегами (quotes.toscrape.com)

```yaml
nested_selectors:
  - repeat_selector: .quote          # ← ищем все .quote
    fields:
      - name: quote_text             # ← внутри .quote ищем .text
        selector: .text
      - name: quote_author           # ← внутри .quote ищем .author
        selector: .author
    nested:                          # ← ВЛОЖЕННЫЙ УРОВЕНЬ
      - repeat_selector: .tags .tag  # ← внутри каждого .quote ищем .tag
        fields:
          - name: tag_name
            selector: "."            # ← текст самого a.tag
```

Результат в JSONL:
```json
{"field":"quote_text[0]","value":"“The world as we have created it...”"}
{"field":"quote_author[0]","value":"Albert Einstein"}
{"field":"tag_name[0.0]","value":"change"}
{"field":"tag_name[0.1]","value":"deep-thoughts"}
{"field":"quote_text[1]","value":"“It is our choices, Harry...”"}
{"field":"tag_name[1.0]","value":"abilities"}
```

Путь `tag_name[1.0]` читается как: цитата №1 → тег №0.

## Циклы подстановки (§4.2)

`start_urls` могут содержать плейсхолдеры `{var}`. `loops` раскрываются в
декартово произведение: для каждой комбинации значений всех циклов генерируется
набор (URL, переменные). Переменные подставляются не только в URL, но и в
заголовки/куки (`headers`, `cookies`).

```yaml
start_urls:
  - https://quotes.toscrape.com/page/{page}/
loops:
  - var: page
    range: { start: 1, end: 5 }        # 1..=5 (step по умолчанию 1)
  # - var: id
  #   values: ["a", "b"]               # либо список значений
```

## Цикл while с авто-границей (§4.3)

`while` фетчит последовательные страницы, инкрементируя переменную `{var}`,
и останавливается, как только выполнено условие `stop_when`:

| Поле `stop_when` | Условие остановки |
| --- | --- |
| `status` | HTTP-статус равен указанному (например, `404`) |
| `text` | тело страницы **содержит** строку |
| `text_not` | тело страницы **НЕ содержит** строку |

```yaml
while:
  var: page
  start: 1
  step: 1
  max_iterations: 100        # защита от бесконечного цикла
  stop_when:
    status: 404
    text: "No quotes found"
```

## output_template

Шаблон JSON-скелета с плейсхолдерами. Применяется только для `--output *.json`.

- `{{field}}` — первое значение поля (любого уровня, включая `tag_name[0.2]`);
- `{{field[*]}}` — все значения поля списком;
- `__each__: "repeat_name"` — секция повторения: для каждого блока `repeat_name[block]`
  собирает объект; внутри неё `{{flat_name}}` подставляется по индексу блока,
  а `{{nested_name}}` собирает все значения `nested_name[block.*]` списком.

```yaml
output_template:
  source: "quotes.toscrape.com"
  title: "{{page_title}}"
  quotes:
    __each__: "quote_text"
    text: "{{quote_text}}"
    author: "{{quote_author}}"
    tags: "{{tag_name}}"
```

## Переход по ссылкам (follow) (§5)

`follow` включает обход по ссылкам: после извлечения данных со страницы
краулер находит ссылки по CSS-селектору, резолвит их относительно текущего
URL (относительные и абсолютные `href`) и ставит в очередь (BFS). Каждая
найденная страница обрабатывается так же: селекторы → записи → снова follow.

| Поле | Описание | По умолчанию |
| --- | --- | --- |
| `selector` | CSS-селектор ссылок: у совпавших элементов берётся `href` | обязателен |
| `max_depth` | Максимальная глубина переходов (1 = одна ступень от стартовых страниц) | `1` |
| `pattern` | Фильтр-подстрока: в очередь идут только ссылки, содержащие её | — |
| `same_host` | Не покидать хост страницы, где найдена ссылка | `true` |

```yaml
start_urls:
  - https://quotes.toscrape.com/
follow:
  selector: "ul.pager a"     # ссылки пагинации
  max_depth: 3
  pattern: "/page/"
  same_host: true
```

Детали поведения:

- **Дедупликация по URL** — страница фетчится не более одного раза;
  не-HTTP схемы (`mailto:`, `javascript:`, ...) отбрасываются при резолве;
- `limit` считает **все** страницы, включая найденные по ссылкам;
- переменные `loops`/`while` **наследуются** найденными страницами
  (подставляются в `headers`/`cookies`);
- ссылки обрабатываются в порядке BFS: сначала все ссылки уровня 1,
  затем уровня 2 и т.д.

## Прокси

```yaml
proxies:
  - http://user:pass@1.2.3.4:8080
  - socks5://5.6.7.8:1080
```
