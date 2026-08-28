# 3. job.yaml — декларативный формат заданий (веха M3)

## Структура

```yaml
start_urls:
  - https://example.com/

selectors:
  - name: page_title
    selector: h1

nested_selectors:
  - repeat_selector: .quote
    fields:
      - name: quote_text
        selector: .text
      - name: quote_author
        selector: .author

limit: 100
```

## Поля

| Поле | Описание |
|---|---|
| `start_urls` | Список URL для начала краулинга |
| `selectors` | Плоские CSS-селекторы: `name` + `selector`. Извлекает первое совпадение |
| `nested_selectors` | Вложенные селекторы для списков (см. ниже) |
| `limit` | Опциональный лимит страниц |

## Вложенные селекторы (§4)

Вложенный поиск позволяет извлекать структурированные данные из повторяющихся
блоков на странице. Алгоритм:

1. **Сначала** ищется родительский блок по `repeat_selector` (например, `.quote`)
2. **Внутри каждого** совпадения ищутся поля по их CSS-селекторам
3. Каждая запись получает `index` — номер блока (0-based)

### Пример

```yaml
nested_selectors:
  - repeat_selector: .quote          # ← ищем все .quote
    fields:
      - name: quote_text             # ← внутри .quote ищем .text
        selector: .text
      - name: quote_author           # ← внутри .quote ищем .author
        selector: .author
```

Результат в JSONL:
```json
{"field":"quote_text[0]","value":"..."}
{"field":"quote_author[0]","value":"..."}
{"field":"quote_text[1]","value":"..."}
{"field":"quote_author[1]","value":"..."}
```

## Прокси

```yaml
proxies:
  - http://user:pass@1.2.3.4:8080
  - socks5://5.6.7.8:1080
```
