use {
    crate::{Kind, TextSize},
    erasable::{Erasable, ErasedPtr},
    slice_dst::{AllocSliceDst, SliceDst},
    std::{alloc::Layout, convert::TryFrom, fmt, hash, ptr, str},
};

/// A leaf token in the immutable green tree.
///
/// Tokens are crated using [`Builder::token`](crate::green::Builder::token).
#[repr(C, align(2))] // NB: align >= 2
#[derive(Eq)]
pub struct Token {
    // NB: This is optimal layout, as the order is (u32, u16, [u8]).
    // SAFETY: Must be at offset 0 and
    //   - accurate to trailing array length, or
    //   - >= 1, the first text byte is 0xFF, and the text array is len 1.
    text_len: TextSize,
    kind: Kind,
    // SAFETY: Must be at offset 6 and
    //  - valid UTF-8, or
    //  - just [0xFF] (which isn't).
    text: [u8],
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("Token");
        d.field("text_len", &self.text_len);
        d.field("kind", &self.kind);
        if let Some(text) = self.text() {
            d.field("text", &text);
        } else {
            d.field("text", &"{unknown}");
        }
        d.finish()
    }
}

// Manually impl Eq/Hash so that builder can spoof it
// Plus we can skip .text_len since it's derived from .text
impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.text == other.text
    }
}

impl hash::Hash for Token {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
        self.text.hash(state);
    }
}

#[allow(clippy::len_without_is_empty)]
impl Token {
    /// The kind of this token.
    #[inline]
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// The text of this token.
    #[inline]
    pub fn text(&self) -> Option<&str> {
        if self.is_thunk() {
            None
        } else {
            Some(unsafe { str::from_utf8_unchecked(&self.text) })
        }
    }

    /// Is this token a Thunk?
    #[inline]
    pub fn is_thunk(&self) -> bool {
        self.text.len() != self.text_len.into()
    }

    /// The raw text of this token, which may be `&[0xFF]` for a thunk.
    #[inline]
    pub(crate) fn raw_text(&self) -> &[u8] {
        &self.text
    }

    /// The length of text at this token.
    #[inline]
    pub fn len(&self) -> TextSize {
        self.text_len
    }

    // SAFETY: must accurately calculate the layout for length `len`
    fn layout(len: usize) -> (Layout, [usize; 3]) {
        let (layout, offset_0) = (Layout::new::<TextSize>(), 0);
        let (layout, offset_1) = layout.extend(Layout::new::<Kind>()).unwrap();
        let (layout, offset_2) = layout.extend(Layout::array::<u8>(len).unwrap()).unwrap();
        // Assert layout assumptions
        debug_assert_eq!(offset_0, 0);
        debug_assert_eq!(offset_1, 4);
        debug_assert_eq!(offset_2, 6);
        (layout.pad_to_align(), [offset_0, offset_1, offset_2])
    }

    #[allow(clippy::new_ret_no_self)]
    pub(super) fn new<A>(kind: Kind, text: &str) -> A
    where
        A: AllocSliceDst<Self>,
    {
        let len = text.len();
        let text_len = TextSize::try_from(len).expect("text too long");
        let (layout, [text_len_offset, kind_offset, text_offset]) = Self::layout(len);

        unsafe {
            // SAFETY: closure fully initializes the place
            A::new_slice_dst(len, |ptr| {
                let raw = ptr.as_ptr().cast::<u8>();
                ptr::write(raw.add(text_len_offset).cast(), text_len);
                ptr::write(raw.add(kind_offset).cast(), kind);
                let text_ptr = raw.add(text_offset);
                ptr::copy_nonoverlapping(text.as_bytes().as_ptr(), text_ptr, len);
                debug_assert_eq!(layout, Layout::for_value(ptr.as_ref()));
            })
        }
    }

    #[allow(clippy::new_ret_no_self)]
    pub(super) fn new_thunk<A>(kind: Kind, len: TextSize) -> A
    where
        A: AllocSliceDst<Self>,
    {
        assert!(len > 0.into());
        let (layout, [text_len_offset, kind_offset, text_offset]) = Self::layout(1);

        unsafe {
            // SAFETY: closure fully initializes the place
            A::new_slice_dst(1, |ptr| {
                let raw = ptr.as_ptr().cast::<u8>();
                ptr::write(raw.add(text_len_offset).cast(), len);
                ptr::write(raw.add(kind_offset).cast(), kind);
                ptr::write(raw.add(text_offset).cast(), 0xFFu8);
                debug_assert_eq!(layout, Layout::for_value(ptr.as_ref()));
            })
        }
    }
}

// SAFETY: un/erase correctly round-trips a pointer
unsafe impl Erasable for Token {
    unsafe fn unerase(this: ErasedPtr) -> ptr::NonNull<Self> {
        // SAFETY: text_len is at offset 0
        let text_len: TextSize = ptr::read(this.cast().as_ptr());
        let tail_len = if text_len > 0.into() {
            // SAFETY: text is at offset 6
            let string_head = ptr::read(this.cast::<u8>().as_ptr().offset(6));
            if string_head == 0xFF {
                1
            } else {
                text_len.into()
            }
        } else {
            text_len.into()
        };
        let ptr = ptr::slice_from_raw_parts_mut(this.as_ptr().cast(), tail_len);
        // SAFETY: ptr comes from NonNull
        Self::retype(ptr::NonNull::new_unchecked(ptr))
    }

    const ACK_1_1_0: bool = true;
}

// SAFETY: layout is correct and retype is the trivial cast
unsafe impl SliceDst for Token {
    fn layout_for(len: usize) -> Layout {
        Self::layout(len).0
    }

    #[allow(clippy::cast_ptr_alignment)]
    fn retype(ptr: ptr::NonNull<[()]>) -> ptr::NonNull<Self> {
        ptr::NonNull::new(ptr.as_ptr() as *mut _).unwrap()
    }
}
