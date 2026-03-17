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
#[derive(Default, Clone)]
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
    /// Create image with component data allocated (C: opj_image_create).
    pub fn new(params: &[ImageCompParam], color_space: ColorSpace) -> Self {
        let comps = params
            .iter()
            .map(|p| {
                let size = (p.w as usize)
                    .checked_mul(p.h as usize)
                    .expect("image component dimensions overflow usize");
                ImageComp {
                    dx: p.dx,
                    dy: p.dy,
                    w: p.w,
                    h: p.h,
                    x0: p.x0,
                    y0: p.y0,
                    prec: p.prec,
                    sgnd: p.sgnd,
                    resno_decoded: 0,
                    factor: 0,
                    data: vec![0; size],
                    alpha: 0,
                }
            })
            .collect();
        Self {
            x0: 0,
            y0: 0,
            x1: 0,
            y1: 0,
            color_space,
            comps,
            icc_profile: Vec::new(),
        }
    }

    /// Create tile image without data allocation (C: opj_image_tile_create).
    pub fn new_tile(params: &[ImageCompParam], color_space: ColorSpace) -> Self {
        let comps = params
            .iter()
            .map(|p| ImageComp {
                dx: p.dx,
                dy: p.dy,
                w: p.w,
                h: p.h,
                x0: p.x0,
                y0: p.y0,
                prec: p.prec,
                sgnd: p.sgnd,
                resno_decoded: 0,
                factor: 0,
                data: Vec::new(),
                alpha: 0,
            })
            .collect();
        Self {
            x0: 0,
            y0: 0,
            x1: 0,
            y1: 0,
            color_space,
            comps,
            icc_profile: Vec::new(),
        }
    }

    /// Copy header and component metadata without data (C: opj_copy_image_header).
    pub fn clone_header(&self) -> Self {
        let comps = self
            .comps
            .iter()
            .map(|c| ImageComp {
                dx: c.dx,
                dy: c.dy,
                w: c.w,
                h: c.h,
                x0: c.x0,
                y0: c.y0,
                prec: c.prec,
                sgnd: c.sgnd,
                resno_decoded: c.resno_decoded,
                factor: c.factor,
                data: Vec::new(),
                alpha: c.alpha,
            })
            .collect();
        Self {
            x0: self.x0,
            y0: self.y0,
            x1: self.x1,
            y1: self.y1,
            color_space: self.color_space,
            comps,
            icc_profile: self.icc_profile.clone(),
        }
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
    fn image_new_creates_components() {
        let params = rgb_params();
        let img = Image::new(&params, ColorSpace::Srgb);
        assert_eq!(img.comps.len(), 3);
        assert_eq!(img.color_space, ColorSpace::Srgb);
    }

    #[test]
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
    fn image_new_tile_no_data() {
        let params = rgb_params();
        let img = Image::new_tile(&params, ColorSpace::Srgb);
        assert_eq!(img.comps.len(), 3);
        for comp in &img.comps {
            assert!(comp.data.is_empty());
        }
    }

    #[test]
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
    fn clone_header_no_data() {
        let params = rgb_params();
        let img = Image::new(&params, ColorSpace::Srgb);
        let cloned = img.clone_header();
        for comp in &cloned.comps {
            assert!(comp.data.is_empty());
        }
    }

    #[test]
    fn clone_header_independent() {
        let params = rgb_params();
        let img = Image::new(&params, ColorSpace::Srgb);
        let mut cloned = img.clone_header();
        cloned.x0 = 999;
        assert_ne!(img.x0, 999);
    }
}
