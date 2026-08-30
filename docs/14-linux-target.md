# 14. Linux как первоклассный таргет (production-деплой)

> Проект разрабатывается на Windows, но **главный production-таргет — Linux** (обычно headless-сервер/VPS). Linux-сборка включена в CI с самого начала, чтобы платформозависимый код писался один раз, а не переписывался на поздних этапах.

## 14.1. Почему Linux

- **Деплой**: headless-сервер без дисплея — основное окружение краулера;
- **минимализм**: статический musl-бинарник «поставил и работает» на любом дистрибутиве без системных пакетов;
- **производительность**: эскалация до eBPF/TUN-TAP (см. [17-ebpf.md](17-ebpf.md)) возможна только на Linux;
- **контейнеры**: Docker/K8s штатно живет на Linux.

## 14.2. Требования к коду для совместимости

- Весь платформозависимый код — за `#[cfg(target_os = "linux")]` / `#[cfg(windows)]` с реализацией для обоих таргетов или явным фоллбэком (правило 7 в [rules.md](rules.md));
- никакого системного OpenSSL — только pure-Rust TLS (`rustls`/`boring`), чтобы musl-сборка была чистой;
- пути — через `std::path`/`camino`, без жёстких разделителей `\`/`/` в строках;
- задержки/тактика — не зависят от названия ОС, используют только `std::time`/`tokio::time`.

## 14.3. Сборка musl (однострочный билд)

```bash
# Тулчейн для статической линковки
rustup target add x86_64-unknown-linux-musl
# Сам билд
cargo build --release --target x86_64-unknown-linux-musl
```

Бинарник: `target/x86_64-unknown-linux-musl/release/arachne` — запускается на любом «голом» контейнере (`scratch`/`alpine`) без установки зависимостей.

## 14.4. Запуск как сервис (systemd)

```ini
# /etc/systemd/system/arachne.service
[Unit]
Description=Arachne stealth crawler
After=network-online.target

[Service]
Type=simple
ExecStart=/opt/arachne/arachne --config /etc/arachne/config.yaml
Restart=on-failure
User=arachne
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
```

## 14.5. Тюнинг ядра (запуск с привилегиями)

Для продвинутых сетевых фич (потребуется root-доступ):
- повышение лимита открытых файлов: `LimitNOFILE` (выше);
- отключение адаптивных таймингов TCP/RST при нестабильном интернете;
- включение opaque-приманок (RFC 5961 `tcp_challenge_ack_limit`) для повышения устойчивости соединений;
- при переходе на eBPF/TUN-TAP — капсибуilities `CAP_NET_ADMIN`, см. [17-ebpf.md](17-ebpf.md).

## 14.6. Чек-лист Linux-таргета

- [ ] Сборка `x86_64-unknown-linux-musl` чистая в CI на каждый PR.
- [ ] Нет зависимостей на системные библиотеки (по `ldd` бинарник статический).
- [ ] Все пути кэша/конфига разрешаются относительно `XDG_CONFIG_HOME`/`--config`, не хардкодятся.
- [ ] Миграция файлов конфигурации Windows→Linux не требует ручной правки настроек путей.