//! Módulo para el control de pantallas LCD 16x2 (controlador HD44780 o compatible)
//! utilizando comunicación en modo de 4 bits con el ESP32.
//!
//! # Diagrama de conexión:
//! - **RS** -> GPIO 15
//! - **EN** -> GPIO 16
//! - **D4** -> GPIO 17
//! - **D5** -> GPIO 18
//! - **D6** -> GPIO 8
//! - **D7** -> GPIO 3
//! - **RW** -> GND (Modo de solo escritura)
//! - **VSS** -> GND
//! - **VDD** -> 5V (o 3.3V según especificación de tu LCD)
//! - **V0** -> Pin central de potenciómetro de 10k (extremos a VDD y GND)
//! - **A (Ánodo luz de fondo)** -> 5V / 3.3V (a través de resistencia de 220Ω o directo si tiene integrada)
//! - **K (Cátodo luz de fondo)** -> GND

#![allow(dead_code)]

use core::fmt;
use esp_idf_svc::hal::delay::Ets;
use esp_idf_svc::hal::gpio::{Output, PinDriver};
use esp_idf_svc::sys::EspError;

// --- Comandos HD44780 ---
pub const LCD_CLEARDISPLAY: u8 = 0x01;
pub const LCD_RETURNHOME: u8 = 0x02;
pub const LCD_ENTRYMODESET: u8 = 0x04;
pub const LCD_DISPLAYCONTROL: u8 = 0x08;
pub const LCD_CURSORSHIFT: u8 = 0x10;
pub const LCD_FUNCTIONSET: u8 = 0x20;
pub const LCD_SETCGRAMADDR: u8 = 0x40;
pub const LCD_SETDDRAMADDR: u8 = 0x80;

// --- Flags de Modo de Entrada (Entry Mode) ---
pub const LCD_ENTRYLEFT: u8 = 0x02;
pub const LCD_ENTRYSHIFTDECREMENT: u8 = 0x00;

// --- Flags de Control de Pantalla (Display Control) ---
pub const LCD_DISPLAYON: u8 = 0x04;
pub const LCD_DISPLAYOFF: u8 = 0x00;
pub const LCD_CURSORON: u8 = 0x02;
pub const LCD_CURSOROFF: u8 = 0x00;
pub const LCD_BLINKON: u8 = 0x01;
pub const LCD_BLINKOFF: u8 = 0x00;

// --- Flags de Desplazamiento (Shift) ---
pub const LCD_DISPLAYMOVE: u8 = 0x08;
pub const LCD_MOVERIGHT: u8 = 0x04;
pub const LCD_MOVELEFT: u8 = 0x00;

// --- Flags de Función (Function Set) ---
pub const LCD_4BITMODE: u8 = 0x00;
pub const LCD_2LINE: u8 = 0x08;
pub const LCD_5X8DOTS: u8 = 0x00;

/// Controlador para la pantalla LCD 16x2 en modo 4 bits.
pub struct Lcd<'a> {
    rs: PinDriver<'a, Output>,
    en: PinDriver<'a, Output>,
    d4: PinDriver<'a, Output>,
    d5: PinDriver<'a, Output>,
    d6: PinDriver<'a, Output>,
    d7: PinDriver<'a, Output>,
    display_control: u8,
}

impl<'a> Lcd<'a> {
    /// Inicializa una nueva instancia de la pantalla LCD y ejecuta la secuencia
    /// de configuración hardware requerida para el modo de 4 bits.
    pub fn new(
        rs: PinDriver<'a, Output>,
        en: PinDriver<'a, Output>,
        d4: PinDriver<'a, Output>,
        d5: PinDriver<'a, Output>,
        d6: PinDriver<'a, Output>,
        d7: PinDriver<'a, Output>,
    ) -> Result<Self, EspError> {
        let mut lcd = Self {
            rs,
            en,
            d4,
            d5,
            d6,
            d7,
            display_control: LCD_DISPLAYON | LCD_CURSOROFF | LCD_BLINKOFF,
        };

        lcd.init()?;
        Ok(lcd)
    }

    /// Ejecuta la secuencia de inicialización estándar del controlador HD44780
    /// para operar en modo de 4 bits.
    pub fn init(&mut self) -> Result<(), EspError> {
        self.rs.set_low()?;
        self.en.set_low()?;

        // Espera mínima de 40 ms tras energizar el módulo
        Ets::delay_ms(50);

        // Secuencia recomendada en el datasheet para reiniciar en modo 4-bits:
        // Paso 1: Enviar 0x03 y esperar más de 4.1 ms
        self.write_nibble(0x03)?;
        Ets::delay_ms(5);

        // Paso 2: Enviar 0x03 y esperar más de 100 µs
        self.write_nibble(0x03)?;
        Ets::delay_us(150);

        // Paso 3: Enviar 0x03 y esperar más de 100 µs
        self.write_nibble(0x03)?;
        Ets::delay_us(150);

        // Paso 4: Cambiar interfaz a modo de 4 bits enviando 0x02
        self.write_nibble(0x02)?;
        Ets::delay_us(150);

        // A partir de aquí la pantalla procesa comandos completos en 2 nibbles
        // Configurar 4 bits, 2 líneas y caracteres de 5x8 puntos (0x28)
        self.command(LCD_FUNCTIONSET | LCD_4BITMODE | LCD_2LINE | LCD_5X8DOTS)?;

        // Encender display según la configuración inicial (Display ON, Cursor OFF, Blink OFF)
        self.command(LCD_DISPLAYCONTROL | self.display_control)?;

        // Limpiar pantalla
        self.clear()?;

        // Configurar modo de entrada: cursor avanza a la derecha, sin auto-scroll (0x06)
        self.command(LCD_ENTRYMODESET | LCD_ENTRYLEFT | LCD_ENTRYSHIFTDECREMENT)?;

        Ok(())
    }

    /// Envía un nibble (4 bits) a las líneas D4..D7 y genera el pulso de validación en EN.
    fn write_nibble(&mut self, nibble: u8) -> Result<(), EspError> {
        if (nibble & 0x01) != 0 {
            self.d4.set_high()?;
        } else {
            self.d4.set_low()?;
        }

        if (nibble & 0x02) != 0 {
            self.d5.set_high()?;
        } else {
            self.d5.set_low()?;
        }

        if (nibble & 0x04) != 0 {
            self.d6.set_high()?;
        } else {
            self.d6.set_low()?;
        }

        if (nibble & 0x08) != 0 {
            self.d7.set_high()?;
        } else {
            self.d7.set_low()?;
        }

        self.pulse_enable()?;
        Ok(())
    }

    /// Genera el pulso de habilitación en el pin EN (> 450 ns según especificación).
    fn pulse_enable(&mut self) -> Result<(), EspError> {
        self.en.set_low()?;
        Ets::delay_us(1);
        self.en.set_high()?;
        Ets::delay_us(2);
        self.en.set_low()?;
        Ets::delay_us(50);
        Ok(())
    }

    /// Envía un byte completo dividido en 2 fases de 4 bits (nibble alto primero).
    fn send(&mut self, value: u8, is_data: bool) -> Result<(), EspError> {
        if is_data {
            self.rs.set_high()?;
        } else {
            self.rs.set_low()?;
        }

        // Enviar nibble superior (bits 7..4)
        self.write_nibble(value >> 4)?;
        // Enviar nibble inferior (bits 3..0)
        self.write_nibble(value & 0x0F)?;

        // Los comandos de borrado y retorno tardan hasta 1.52 ms en completarse
        if !is_data && (value == LCD_CLEARDISPLAY || value == LCD_RETURNHOME) {
            Ets::delay_ms(2);
        }

        Ok(())
    }

    /// Envía un byte de comando a la pantalla LCD (RS = LOW).
    pub fn command(&mut self, cmd: u8) -> Result<(), EspError> {
        self.send(cmd, false)
    }

    /// Envía un byte de datos/carácter a la pantalla LCD (RS = HIGH).
    pub fn write_byte(&mut self, data: u8) -> Result<(), EspError> {
        self.send(data, true)
    }

    /// Escribe un único carácter en la pantalla en la posición actual del cursor.
    pub fn print_char(&mut self, c: char) -> Result<(), EspError> {
        self.write_byte(c as u8)
    }

    /// Escribe una cadena de texto en la pantalla LCD.
    /// Si encuentra el carácter de salto de línea `\n`, mueve el cursor a la fila 1 (segunda línea).
    pub fn print(&mut self, text: &str) -> Result<(), EspError> {
        for b in text.bytes() {
            if b == b'\n' {
                self.set_cursor(0, 1)?;
            } else {
                self.write_byte(b)?;
            }
        }
        Ok(())
    }

    /// Limpia toda la pantalla y coloca el cursor en la posición (0, 0).
    pub fn clear(&mut self) -> Result<(), EspError> {
        self.command(LCD_CLEARDISPLAY)
    }

    /// Regresa el cursor al inicio (0, 0) sin borrar el texto.
    pub fn home(&mut self) -> Result<(), EspError> {
        self.command(LCD_RETURNHOME)
    }

    /// Posiciona el cursor en la columna (`col`: 0 a 15) y fila (`row`: 0 ó 1).
    pub fn set_cursor(&mut self, col: u8, row: u8) -> Result<(), EspError> {
        let row_offsets = [0x00, 0x40, 0x14, 0x54];
        let row_idx = (row as usize).min(row_offsets.len() - 1);
        let address = row_offsets[row_idx] + col;
        self.command(LCD_SETDDRAMADDR | address)
    }

    /// Enciende la pantalla (muestra el contenido visible).
    pub fn display_on(&mut self) -> Result<(), EspError> {
        self.display_control |= LCD_DISPLAYON;
        self.command(LCD_DISPLAYCONTROL | self.display_control)
    }

    /// Apaga la pantalla (oculta el contenido sin borrar la memoria DDRAM).
    pub fn display_off(&mut self) -> Result<(), EspError> {
        self.display_control &= !LCD_DISPLAYON;
        self.command(LCD_DISPLAYCONTROL | self.display_control)
    }

    /// Muestra la línea de cursor (subrayado).
    pub fn cursor_on(&mut self) -> Result<(), EspError> {
        self.display_control |= LCD_CURSORON;
        self.command(LCD_DISPLAYCONTROL | self.display_control)
    }

    /// Oculta la línea de cursor.
    pub fn cursor_off(&mut self) -> Result<(), EspError> {
        self.display_control &= !LCD_CURSORON;
        self.command(LCD_DISPLAYCONTROL | self.display_control)
    }

    /// Activa el parpadeo del cursor.
    pub fn blink_on(&mut self) -> Result<(), EspError> {
        self.display_control |= LCD_BLINKON;
        self.command(LCD_DISPLAYCONTROL | self.display_control)
    }

    /// Desactiva el parpadeo del cursor.
    pub fn blink_off(&mut self) -> Result<(), EspError> {
        self.display_control &= !LCD_BLINKON;
        self.command(LCD_DISPLAYCONTROL | self.display_control)
    }

    /// Desplaza todo el contenido de la pantalla una posición a la izquierda.
    pub fn scroll_left(&mut self) -> Result<(), EspError> {
        self.command(LCD_CURSORSHIFT | LCD_DISPLAYMOVE | LCD_MOVELEFT)
    }

    /// Desplaza todo el contenido de la pantalla una posición a la derecha.
    pub fn scroll_right(&mut self) -> Result<(), EspError> {
        self.command(LCD_CURSORSHIFT | LCD_DISPLAYMOVE | LCD_MOVERIGHT)
    }

    /// Crea un carácter personalizado en la memoria CGRAM (posiciones 0 a 7).
    /// Cada carácter está compuesto por 8 bytes (cada byte representa una fila de 5 bits).
    pub fn create_char(&mut self, location: u8, pattern: [u8; 8]) -> Result<(), EspError> {
        let loc = location & 0x07;
        self.command(LCD_SETCGRAMADDR | (loc << 3))?;
        for &byte in pattern.iter() {
            self.write_byte(byte)?;
        }
        // Restaurar puntero de memoria a la zona de datos (DDRAM)
        self.command(LCD_SETDDRAMADDR)?;
        Ok(())
    }

    /// Permite utilizar directamente la macro `write!(lcd, "Texto: {}", valor)?`
    /// retornando `Result<(), EspError>` para usar el operador `?` directamente.
    pub fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> Result<(), EspError> {
        fmt::write(self, args).map_err(|_| EspError::from_infallible::<{ esp_idf_svc::sys::ESP_FAIL }>())
    }
}

/// Implementación del trait `fmt::Write` para permitir el uso de macros
/// estándar como `write!(lcd, "Valor: {}", 42)` o `writeln!`.
impl<'a> fmt::Write for Lcd<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.print(s).map_err(|_| fmt::Error)
    }
}
