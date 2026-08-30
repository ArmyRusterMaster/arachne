# Backlog пожеланий и идей

## 1. Переход по ссылкам и нажатие по кнопкам + интеграция в jobs

- [x] **Переход по ссылкам** — реализовано в Phase A как секция `follow` в
  job.yaml: BFS-обход, `selector` / `max_depth` / `pattern` / `same_host`,
  дедупликация по URL, наследование переменных `loops`/`while`.
  Документация: docs/03-job-yaml.md §5, пример — job.yaml.
- [ ] **Нажатие по кнопкам / формы / JS** — в Phase A (Fast HTTP) невозможно
  по определению. План — Фаза B: `Session<Headless>` + `escalate()`
  (rules.md §3), arachne-js (boa) + собственный DOM
  (docs/11-browser-engines.md). Интеграция в job.yaml — после появления
  engine-слоя (планируется секция `actions:` в job).

## 2. Свой DSL для сценариев парсинга (вне фаз)

- Статус: **отложено, вне фаз** — по docs/19-saas-platform.md DSL/low-code
  конструктор относится к SaaS-слою (блоки HTTP GET / CLICK / FILL /
  EXTRACT / LOOP / IF / PAGINATE, сценарий сохраняется как YAML).
- Промежуточный шаг уже есть: декларативный job.yaml (веха M3) покрывает
  простые сценарии без DSL: selectors / nested_selectors / loops / while /
  output_template / follow.