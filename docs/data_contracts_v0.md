# Data Contracts v0 (Этап 1)

Источник: `docs/phase1_plan.md`.

## Минимальный состав сущностей и связей

- `Song` содержит 8 `Track`.
- `Track` ссылается на `Chain` по строкам song.
- `Chain` содержит последовательность `Phrase` (до 16 строк).
- `Phrase` содержит 16 шагов: `Note`, `Velocity`, `Instrument`, `FX1..FX3`.
- `Instrument` может использовать `Table` и маршрутизируется в `Mixer`.
- `Table` содержит шаговые модификации и FX-команды.
- `Groove` задаёт длительность шагов (ticks).
- `Scale` задаёт тональность/квантизацию.
- `Mixer` управляет уровнями/посылами/мастером.
- `FX` — командный слой автоматизации/изменений параметров.

## Обязательные поля v0

- `Song`: `name`, `tempo`, `default_groove`, `default_scale`.
- `Track`: `index`, `song_rows[]` (ссылки на chain), `mute`, `solo`.
- `Chain`: `id`, `rows[]` (phrase refs + transpose).
- `Phrase`: `id`, `steps[16]`.
- `Step`: `note`, `velocity`, `instrument_id`, `fx[3]`.
- `Instrument`: `id`, `type`, `name`, `send_levels`, `table_id`.
- `Table`: `id`, `rows[16]`.
- `Groove`: `id`, `ticks_pattern`.
- `Scale`: `id`, `key`, `interval_mask`.
- `Mixer`: `track_levels[8]`, `master_level`, `send_levels`.

## Deferred (позже)

- Расширенные modulation-режимы.
- Полный набор FX-команд и вероятностных сценариев.
- Продвинутые режимы live-performance.
- Расширенная экосистема файлов/тем/интеграций.
