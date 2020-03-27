use {
    crate::{Kind, TextSize},
    erasable::{Erasable, ErasedPtr},
    slice_dst::{AllocSliceDst, SliceDst},
    std::{alloc::Layout, convert::TryFrom, ptr},
};

/// A leaf token in the immutable green tree.
///
/// Tokens are crated using [`Builder::token`](crate::green::Builder::token).
#[repr(C, align(2))] // NB: align >= 2
#[derive(Debug, Eq, PartialEq, Hash)]
pub struct Token {
    // NB: This is optimal layout, as the order is (u32, u16, [u8]).
    // SAFETY: Must be at offset 0 and accurate to trailing array length.
    text_len: TextSize,
    kind: Kind,
    text: str,
}

#[allow(clippy::len_without_is_empty)]
impl Token {
    /// The kind of this token.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// The text of this token.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The length of text at this token.
    pub fn len(&self) -> TextSize {
        self.text_len
    }

    // SAFETY: must accurately calculate the layout for length `len`
    fn layout(len: usize) -> (Layout, [usize; 3]) {
        let (layout, offset_0) = (Layout::new::<TextSize>(), 0);
        let (layout, offset_1) = layout.extend(Layout::new::<Kind>()).unwrap();
        let (layout, offset_2) = layout.extend(Layout::array::<u8>(len).unwrap()).unwrap();
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
}

// SAFETY: un/erase correctly round-trips a pointer
unsafe impl Erasable for Token {
    unsafe fn unerase(this: ErasedPtr) -> ptr::NonNull<Self> {
        // SAFETY: text_len is at 0 offset
        let text_len: TextSize = ptr::read(this.cast().as_ptr());
        let ptr = ptr::slice_from_raw_parts_mut(this.as_ptr().cast(), text_len.into());
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
