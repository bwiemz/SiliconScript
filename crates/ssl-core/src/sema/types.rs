use std::fmt;

// ---------------------------------------------------------------------------
// Opaque ID types for user-defined types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InterfaceId(pub u32);

// ---------------------------------------------------------------------------
// Ty — the compiler's resolved type representation
// ---------------------------------------------------------------------------

/// A fully-resolved SiliconScript type with concrete widths.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    // ------------------------------------------------------------------
    // Hardware primitive types
    // ------------------------------------------------------------------
    /// Unsigned integer bit-vector of width `N`.
    UInt(u64),
    /// Signed (two's complement) integer bit-vector of width `N`.
    SInt(u64),
    /// Raw bit-vector with no arithmetic interpretation.
    Bits(u64),
    /// Fixed-point number: `int_bits` integer bits + `frac_bits` fractional bits.
    Fixed { int_bits: u64, frac_bits: u64 },
    /// Single-bit boolean (distinct from `UInt<1>`).
    Bool,
    /// Clock signal; optional known frequency in Hz.
    Clock { freq: Option<u64> },
    /// Synchronous reset.
    SyncReset,
    /// Asynchronous reset.
    AsyncReset,

    // ------------------------------------------------------------------
    // Compound / aggregate types
    // ------------------------------------------------------------------
    /// Fixed-size homogeneous array.
    Array { element: Box<Ty>, size: u64 },
    /// User-defined struct (resolved by ID).
    Struct(StructId),
    /// User-defined enum (resolved by ID).
    Enum(EnumId),
    /// Named interface bundle (resolved by ID).
    Interface(InterfaceId),
    /// Memory primitive (ROM/RAM).
    Memory { element: Box<Ty>, depth: u64 },

    // ------------------------------------------------------------------
    // Direction wrappers (used on interface signal types only)
    // ------------------------------------------------------------------
    In(Box<Ty>),
    Out(Box<Ty>),
    InOut(Box<Ty>),
    /// Flip all port directions recursively.
    Flip(Box<Ty>),

    // ------------------------------------------------------------------
    // Compile-time meta / elaboration types
    // ------------------------------------------------------------------
    /// Unsized non-negative integer literal (before width inference).
    MetaUInt,
    /// Unsized signed integer literal (before width inference).
    MetaInt,
    /// Boolean literal type.
    MetaBool,
    /// Floating-point literal used in elaboration arithmetic.
    MetaFloat,
    /// String literal type.
    MetaString,
    /// Type of a type expression (e.g. a generic parameter).
    MetaType,

    // ------------------------------------------------------------------
    // Special sentinel types
    // ------------------------------------------------------------------
    /// Error sentinel — used to propagate type errors without cascading.
    Error,
    /// Unit / no-value type.
    Void,
}

// ---------------------------------------------------------------------------
// Methods
// ---------------------------------------------------------------------------

impl Ty {
    /// Return the hardware bit-width of the type, or `None` for meta types,
    /// error sentinels, user-defined aggregates (width unknown without context),
    /// and `Void`.
    pub fn bit_width(&self) -> Option<u64> {
        match self {
            Ty::UInt(n) | Ty::SInt(n) | Ty::Bits(n) => Some(*n),
            Ty::Fixed { int_bits, frac_bits } => Some(int_bits + frac_bits),
            Ty::Bool | Ty::Clock { .. } | Ty::SyncReset | Ty::AsyncReset => Some(1),
            Ty::Array { element, size } => {
                element.bit_width().map(|w| w * size)
            }
            Ty::Memory { element, depth } => {
                element.bit_width().map(|w| w * depth)
            }
            Ty::In(inner) | Ty::Out(inner) | Ty::InOut(inner) | Ty::Flip(inner) => {
                inner.bit_width()
            }
            // User-defined aggregates: width is not known from the type tag alone.
            Ty::Struct(_) | Ty::Enum(_) | Ty::Interface(_) => None,
            // Meta, error, void: no hardware width.
            Ty::MetaUInt
            | Ty::MetaInt
            | Ty::MetaBool
            | Ty::MetaFloat
            | Ty::MetaString
            | Ty::MetaType
            | Ty::Error
            | Ty::Void => None,
        }
    }

    /// True if the type supports arithmetic operations: `UInt`, `SInt`, `Bits`, `Fixed`.
    pub fn is_numeric(&self) -> bool {
        matches!(self, Ty::UInt(_) | Ty::SInt(_) | Ty::Bits(_) | Ty::Fixed { .. })
    }

    /// True for signed/unsigned integer types only (`UInt`, `SInt`).
    pub fn is_integer(&self) -> bool {
        matches!(self, Ty::UInt(_) | Ty::SInt(_))
    }

    /// True if the type maps to synthesizable hardware (i.e. can appear in a
    /// module's port list or signal declarations, excluding meta/error/void).
    pub fn is_synthesizable(&self) -> bool {
        match self {
            Ty::UInt(_)
            | Ty::SInt(_)
            | Ty::Bits(_)
            | Ty::Fixed { .. }
            | Ty::Bool
            | Ty::Clock { .. }
            | Ty::SyncReset
            | Ty::AsyncReset
            | Ty::Array { .. }
            | Ty::Struct(_)
            | Ty::Enum(_)
            | Ty::Interface(_)
            | Ty::Memory { .. }
            | Ty::In(_)
            | Ty::Out(_)
            | Ty::InOut(_)
            | Ty::Flip(_) => true,

            Ty::MetaUInt
            | Ty::MetaInt
            | Ty::MetaBool
            | Ty::MetaFloat
            | Ty::MetaString
            | Ty::MetaType
            | Ty::Error
            | Ty::Void => false,
        }
    }

    /// True for compile-time-only meta types.
    pub fn is_meta(&self) -> bool {
        matches!(
            self,
            Ty::MetaUInt | Ty::MetaInt | Ty::MetaBool | Ty::MetaFloat | Ty::MetaString | Ty::MetaType
        )
    }

    /// True if this is the error sentinel type.
    pub fn is_error(&self) -> bool {
        matches!(self, Ty::Error)
    }

    /// Strip a single layer of direction wrapper (`In`, `Out`, `InOut`, `Flip`).
    /// Returns `self` unchanged for non-direction types.
    pub fn unwrap_direction(&self) -> &Ty {
        match self {
            Ty::In(inner) | Ty::Out(inner) | Ty::InOut(inner) | Ty::Flip(inner) => inner,
            other => other,
        }
    }
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::UInt(n) => write!(f, "UInt<{n}>"),
            Ty::SInt(n) => write!(f, "SInt<{n}>"),
            Ty::Bits(n) => write!(f, "Bits<{n}>"),
            Ty::Fixed { int_bits, frac_bits } => write!(f, "Fixed<{int_bits}, {frac_bits}>"),
            Ty::Bool => write!(f, "Bool"),
            Ty::Clock { freq: None } => write!(f, "Clock"),
            Ty::Clock { freq: Some(hz) } => write!(f, "Clock<{hz}>"),
            Ty::SyncReset => write!(f, "SyncReset"),
            Ty::AsyncReset => write!(f, "AsyncReset"),
            Ty::Array { element, size } => write!(f, "[{element}; {size}]"),
            Ty::Struct(StructId(id)) => write!(f, "Struct#{id}"),
            Ty::Enum(EnumId(id)) => write!(f, "Enum#{id}"),
            Ty::Interface(InterfaceId(id)) => write!(f, "Interface#{id}"),
            Ty::Memory { element, depth } => write!(f, "Memory<{element}, {depth}>"),
            Ty::In(inner) => write!(f, "In<{inner}>"),
            Ty::Out(inner) => write!(f, "Out<{inner}>"),
            Ty::InOut(inner) => write!(f, "InOut<{inner}>"),
            Ty::Flip(inner) => write!(f, "Flip<{inner}>"),
            Ty::MetaUInt => write!(f, "MetaUInt"),
            Ty::MetaInt => write!(f, "MetaInt"),
            Ty::MetaBool => write!(f, "MetaBool"),
            Ty::MetaFloat => write!(f, "MetaFloat"),
            Ty::MetaString => write!(f, "MetaString"),
            Ty::MetaType => write!(f, "MetaType"),
            Ty::Error => write!(f, "<error>"),
            Ty::Void => write!(f, "Void"),
        }
    }
}
