# cpal + PipeWire: Sample Rate Bouncing

## Симптом

При старте синта в stderr:
```
Audio stream error: A backend-specific error has occurred: sample rate changed to: 48000
Audio stream error: A backend-specific error has occurred: sample rate changed to: 44100
Audio stream error: A backend-specific error has occurred: sample rate changed to: 48000
```

## Причина

**cpal 0.15.3 JACK backend + PipeWire JACK compatibility layer.**

1. Наш синт подключается как JACK-клиент через cpal
2. PipeWire при активации клиента пересогласовывает sample rate аудио-графа
3. Если `allowed-rates` содержит несколько значений (44100, 48000), PipeWire прыгает между ними пока граф устаканивается
4. Каждый прыжок — JACK `sample_rate` callback → cpal шлёт в error callback

## Проблема в cpal

- Первый `sample_rate` callback — cpal игнорирует (startup)
- Все последующие — cpal возвращает `jack::Control::Quit` (должен убить стрим)
- Стрим продолжает работать только потому что PipeWire JACK shim не всегда реагирует на quit
- В cpal 0.17.1 поведение изменилось: вместо строки шлёт `StreamError::StreamInvalidated`

## Наш workaround (audio.rs)

Перехватываем error callback, парсим строку "sample rate changed to: {N}",
обновляем AtomicU32, synth engine подхватывает новый SR.
150ms sleep после stream.play() даёт PipeWire время устаканиться.

## Решения

### Вариант 1: Зафиксировать rate в PipeWire

`~/.config/pipewire/pipewire.conf.d/10-fixed-rate.conf`:
```
context.properties = {
    default.clock.rate = 48000
    default.clock.allowed-rates = [48000]
}
```

### Вариант 2: Использовать ALSA backend вместо JACK

В настройках синта переключить audio backend на ALSA.
Тогда PipeWire JACK shim не задействуется.

### Вариант 3: Обновить cpal

В 0.17.1 ошибка чище (`StreamInvalidated`), но суть та же.
Полноценного решения в cpal нет — JACK API не предусматривает
"мягкую" обработку rate change.

## Влияние на звук

Нет. Финальный SR всегда правильный. Сообщения — косметические.
