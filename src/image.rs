use crate::types::ColorSpace;

/// Component creation parameters (C: opj_image_cmptparm_t).
pub struct ImageCompParam {
    pub dx: u32,
    pub dy: u32,
    pub w: u32,
    pub h: u32,
    pub x0: u32,
    pub y0: u32,
    pub prec: u32,
    pub sgnd: bool,
}

/// Image component (C: opj_image_comp_t).
pub struct ImageComp {
    pub dx: u32,
    pub dy: u32,
    pub w: u32,
    pub h: u32,
    pub x0: u32,
    pub y0: u32,
    pub prec: u32,
    pub sgnd: bool,
    pub resno_decoded: u32,
    pub factor: u32,
    pub data: Vec<i32>,
    pub alpha: u16,
}

/// Image data structure (C: opj_image_t).
pub struct Image {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
    pub color_space: ColorSpace,
    pub comps: Vec<ImageComp>,
    pub icc_profile: Vec<u8>,
}

impl Image {
    pub fn new(_params: &[ImageCompParam], _color_space: ColorSpace) -> Self {
        todo!()
    }

    pub fn new_tile(_params: &[ImageCompParam], _color_space: ColorSpace) -> Self {
        todo!()
    }

    pub fn clone_header(&self) -> Self {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb_params() -> Vec<ImageCompParam> {
        (0..3)
            .map(|_| ImageCompParam {
                dx: 1,
                dy: 1,
                w: 640,
                h: 480,
                x0: 0,
                y0: 0,
                prec: 8,
                sgnd: false,
            })
            .collect()
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn image_new_creates_components() {
        let params = rgb_params();
        let img = Image::new(&params, ColorSpace::Srgb);
        assert_eq!(img.comps.len(), 3);
        assert_eq!(img.color_space, ColorSpace::Srgb);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn image_new_allocates_data() {
        let params = rgb_params();
        let img = Image::new(&params, ColorSpace::Srgb);
        for comp in &img.comps {
            assert_eq!(comp.data.len(), 640 * 480);
            assert_eq!(comp.dx, 1);
            assert_eq!(comp.dy, 1);
            assert_eq!(comp.prec, 8);
            assert!(!comp.sgnd);
        }
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn image_new_tile_no_data() {
        let params = rgb_params();
        let img = Image::new_tile(&params, ColorSpace::Srgb);
        assert_eq!(img.comps.len(), 3);
        for comp in &img.comps {
            assert!(comp.data.is_empty());
        }
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn clone_header_copies_metadata() {
        let params = rgb_params();
        let mut img = Image::new(&params, ColorSpace::Srgb);
        img.x0 = 10;
        img.y0 = 20;
        img.x1 = 650;
        img.y1 = 500;
        img.icc_profile = vec![1, 2, 3, 4];

        let cloned = img.clone_header();
        assert_eq!(cloned.x0, 10);
        assert_eq!(cloned.y0, 20);
        assert_eq!(cloned.x1, 650);
        assert_eq!(cloned.y1, 500);
        assert_eq!(cloned.color_space, ColorSpace::Srgb);
        assert_eq!(cloned.comps.len(), 3);
        assert_eq!(cloned.icc_profile, vec![1, 2, 3, 4]);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn clone_header_no_data() {
        let params = rgb_params();
        let img = Image::new(&params, ColorSpace::Srgb);
        let cloned = img.clone_header();
        for comp in &cloned.comps {
            assert!(comp.data.is_empty());
        }
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn clone_header_independent() {
        let params = rgb_params();
        let img = Image::new(&params, ColorSpace::Srgb);
        let mut cloned = img.clone_header();
        cloned.x0 = 999;
        assert_ne!(img.x0, 999);
    }
}
