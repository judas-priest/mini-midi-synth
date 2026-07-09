# Tech Debt

## Audio device auto-reconnect (Android)

**Проблема:** При подключении/отключении наушников аудио стрим умирает. Сейчас показывается toast "restart app".

**Причина:** `SynthEngine` moved в cpal callback closure. При disconnect стрим мёртв, synth engine внутри — нельзя достать и пересоздать стрим.

**Решение:** Обернуть `SynthEngine` в `Arc<Mutex>`, чтобы callback брал lock на каждом вызове. При disconnect — пересоздать cpal stream с тем же `Arc<Mutex<SynthEngine>>`.

**Overhead:** Mutex lock без contention = 3-4ns. На buffer 256 сэмплов @ 48kHz = 5.3ms. Overhead 0.00008%.

**Риски:** Теоретически mutex в RT callback — bad practice. На практике contention = 0 (GUI thread не трогает synth engine через mutex). С `panic = "abort"` deadlock невозможен.

**Затрагивает:** `src/audio.rs`, `src/main.rs`. Десктопную версию тоже — `Arc<Mutex>` будет общий.

**Файлы для изменения:**
- `src/audio.rs` — callback берёт synth через `Arc<Mutex<SynthEngine>>`, добавить `restart()`
- `src/main.rs` — при disconnect вызывать restart вместо toast

**Объём:** ~50-80 строк, 2 файла.
