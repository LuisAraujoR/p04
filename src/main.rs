mod lcd;

use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{InterruptType, PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::sys::EspError;
use esp_idf_svc::timer::EspTimerService;
use lcd::Lcd;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

// Banderas atómicas para comunicar las interrupciones de los botones
static BOTON_1_PRESIONADO: AtomicBool = AtomicBool::new(false);
static BOTON_2_PRESIONADO: AtomicBool = AtomicBool::new(false);
static BOTON_3_PRESIONADO: AtomicBool = AtomicBool::new(false);
static BOTON_4_PRESIONADO: AtomicBool = AtomicBool::new(false);

// Modos del reloj
pub const MODE_NORMAL: u8 = 0;
pub const MODE_HOUR: u8 = 1;
pub const MODE_MINUTE: u8 = 2;
pub const MODE_SECOND: u8 = 3;

// Variables atómicas compartidas entre la tarea de botones y la tarea del reloj/pantalla
static MODO: AtomicU8 = AtomicU8::new(MODE_NORMAL);
static HORA: AtomicU8 = AtomicU8::new(12);
static MINUTO: AtomicU8 = AtomicU8::new(30);
static SEGUNDO: AtomicU8 = AtomicU8::new(45);
static REFRESCAR_LCD: AtomicBool = AtomicBool::new(true);

fn main() -> Result<(), EspError> {
    // Necesario para enlazar parches en tiempo de ejecución del ESP-IDF
    esp_idf_svc::sys::link_patches();

    // Inicializar el sistema de logs del ESP-IDF
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Iniciando aplicación de reloj con tareas concurrentes (LCD y Botones)...");

    // Obtener los periféricos del microcontrolador
    let peripherals = Peripherals::take()?;

    // Configuración de los pines asignados para la LCD (Modo 4 bits):
    // - RS:  GPIO 15
    // - EN:  GPIO 16
    // - D4:  GPIO 17
    // - D5:  GPIO 18
    // - D6:  GPIO 8
    // - D7:  GPIO 3
    let rs = PinDriver::output(peripherals.pins.gpio15)?;
    let en = PinDriver::output(peripherals.pins.gpio16)?;
    let d4 = PinDriver::output(peripherals.pins.gpio17)?;
    let d5 = PinDriver::output(peripherals.pins.gpio18)?;
    let d6 = PinDriver::output(peripherals.pins.gpio8)?;
    let d7 = PinDriver::output(peripherals.pins.gpio3)?;

    // Inicializar la LCD
    log::info!("Inicializando pantalla LCD...");
    let mut lcd = Lcd::new(rs, en, d4, d5, d6, d7)?;
    lcd.clear()?;
    log::info!("LCD inicializada correctamente.");

    // Configuración de los botones con pull-up e interrupción por flanco de bajada
    let mut boton1 = PinDriver::input(peripherals.pins.gpio4, Pull::Up)?;
    let mut boton2 = PinDriver::input(peripherals.pins.gpio5, Pull::Up)?;
    let mut boton3 = PinDriver::input(peripherals.pins.gpio6, Pull::Up)?;
    let mut boton4 = PinDriver::input(peripherals.pins.gpio7, Pull::Up)?;

    boton1.set_interrupt_type(InterruptType::NegEdge)?;
    boton2.set_interrupt_type(InterruptType::NegEdge)?;
    boton3.set_interrupt_type(InterruptType::NegEdge)?;
    boton4.set_interrupt_type(InterruptType::NegEdge)?;

    unsafe {
        boton1.subscribe(|| {
            BOTON_1_PRESIONADO.store(true, Ordering::Relaxed);
        })?;
        boton2.subscribe(|| {
            BOTON_2_PRESIONADO.store(true, Ordering::Relaxed);
        })?;
        boton3.subscribe(|| {
            BOTON_3_PRESIONADO.store(true, Ordering::Relaxed);
        })?;
        boton4.subscribe(|| {
            BOTON_4_PRESIONADO.store(true, Ordering::Relaxed);
        })?;
    }

    boton1.enable_interrupt()?;
    boton2.enable_interrupt()?;
    boton3.enable_interrupt()?;
    boton4.enable_interrupt()?;

    // =========================================================================
    // TAREA DE BOTONES: Maneja la lógica de entrada, rebotes y modificación de estado
    // =========================================================================
    thread::Builder::new()
        .name("tarea_botones".to_string())
        .stack_size(4096)
        .spawn(move || {
            log::info!("Tarea de botones iniciada en segundo plano.");

            loop {
                // Botón 1: Cambiar modo (Normal -> Hora -> Minuto -> Segundo -> Normal)
                if BOTON_1_PRESIONADO.swap(false, Ordering::Relaxed) {
                    let actual = MODO.load(Ordering::Relaxed);
                    let siguiente = (actual + 1) % 4;
                    MODO.store(siguiente, Ordering::Relaxed);
                    REFRESCAR_LCD.store(true, Ordering::Relaxed);

                    FreeRtos::delay_ms(150);
                    let _ = boton1.enable_interrupt();
                }

                // Botón 2: Incrementar valor según el modo activo
                if BOTON_2_PRESIONADO.swap(false, Ordering::Relaxed) {
                    let modo = MODO.load(Ordering::Relaxed);
                    if modo == MODE_HOUR {
                        let h = (HORA.load(Ordering::Relaxed) + 1) % 24;
                        HORA.store(h, Ordering::Relaxed);
                    } else if modo == MODE_MINUTE {
                        let m = (MINUTO.load(Ordering::Relaxed) + 1) % 60;
                        MINUTO.store(m, Ordering::Relaxed);
                    } else if modo == MODE_SECOND {
                        let s = (SEGUNDO.load(Ordering::Relaxed) + 1) % 60;
                        SEGUNDO.store(s, Ordering::Relaxed);
                    }

                    REFRESCAR_LCD.store(true, Ordering::Relaxed);
                    FreeRtos::delay_ms(150);
                    let _ = boton2.enable_interrupt();
                }

                // Botón 3: Decrementar valor según el modo activo
                if BOTON_3_PRESIONADO.swap(false, Ordering::Relaxed) {
                    let modo = MODO.load(Ordering::Relaxed);
                    if modo == MODE_HOUR {
                        let h = HORA.load(Ordering::Relaxed);
                        let nuevo_h = if h == 0 { 23 } else { h - 1 };
                        HORA.store(nuevo_h, Ordering::Relaxed);
                    } else if modo == MODE_MINUTE {
                        let m = MINUTO.load(Ordering::Relaxed);
                        let nuevo_m = if m == 0 { 59 } else { m - 1 };
                        MINUTO.store(nuevo_m, Ordering::Relaxed);
                    } else if modo == MODE_SECOND {
                        let s = SEGUNDO.load(Ordering::Relaxed);
                        let nuevo_s = if s == 0 { 59 } else { s - 1 };
                        SEGUNDO.store(nuevo_s, Ordering::Relaxed);
                    }

                    REFRESCAR_LCD.store(true, Ordering::Relaxed);
                    FreeRtos::delay_ms(150);
                    let _ = boton3.enable_interrupt();
                }

                // Botón 4: Resetear modo a Normal
                if BOTON_4_PRESIONADO.swap(false, Ordering::Relaxed) {
                    MODO.store(MODE_NORMAL, Ordering::Relaxed);
                    REFRESCAR_LCD.store(true, Ordering::Relaxed);

                    FreeRtos::delay_ms(150);
                    let _ = boton4.enable_interrupt();
                }

                FreeRtos::delay_ms(20);
            }
        })
        .expect("Error al iniciar la tarea de botones");

    // =========================================================================
    // TEMPORIZADOR Y TAREA DE INCREMENTO DE HORA (Sin delays en la tarea)
    // =========================================================================
    // Canal sin búfer para enviar el pulso del temporizador a la tarea del reloj
    let (tx_reloj, rx_reloj) = mpsc::sync_channel::<()>(1);
    let timer_service = EspTimerService::new()?;
    let _timer = timer_service.timer(move || {
        let _ = tx_reloj.try_send(());
    })?;
    _timer.every(Duration::from_secs(1))?;

    thread::Builder::new()
        .name("tarea_reloj".to_string())
        .stack_size(4096)
        .spawn(move || {
            log::info!("Tarea de incremento de hora iniciada (activada por temporizador).");

            // Queda bloqueada esperando el pulso de 1 segundo del temporizador, sin consumir CPU ni usar delay
            while rx_reloj.recv().is_ok() {
                if MODO.load(Ordering::Relaxed) == MODE_NORMAL {
                    let mut s = SEGUNDO.load(Ordering::Relaxed) + 1;
                    let mut m = MINUTO.load(Ordering::Relaxed);
                    let mut h = HORA.load(Ordering::Relaxed);

                    if s >= 60 {
                        s = 0;
                        m += 1;
                        if m >= 60 {
                            m = 0;
                            h = (h + 1) % 24;
                        }
                    }

                    SEGUNDO.store(s, Ordering::Relaxed);
                    MINUTO.store(m, Ordering::Relaxed);
                    HORA.store(h, Ordering::Relaxed);
                    REFRESCAR_LCD.store(true, Ordering::Relaxed);
                }
            }
        })
        .expect("Error al iniciar la tarea de incremento de hora");

    // =========================================================================
    // TAREA PRINCIPAL: Manejo y refresco de la pantalla LCD
    // =========================================================================
    loop {
        // Refrescar la pantalla únicamente cuando haya cambios (temporizador o botones)
        if REFRESCAR_LCD.swap(false, Ordering::Relaxed) {
            let h = HORA.load(Ordering::Relaxed);
            let m = MINUTO.load(Ordering::Relaxed);
            let s = SEGUNDO.load(Ordering::Relaxed);
            let modo = MODO.load(Ordering::Relaxed);

            lcd.set_cursor(0, 0)?;
            write!(lcd, "Hora:   {:02}:{:02}:{:02}", h, m, s)?;

            lcd.set_cursor(0, 1)?;
            match modo {
                MODE_NORMAL => write!(lcd, "Modo:   Normal ")?,
                MODE_HOUR => write!(lcd, "Modo:   AjusteH")?,
                MODE_MINUTE => write!(lcd, "Modo:   AjusteM")?,
                MODE_SECOND => write!(lcd, "Modo:   AjusteS")?,
                _ => write!(lcd, "Modo:   Error")?,
            };
        }

        FreeRtos::delay_ms(30);
    }
}
