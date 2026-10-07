use gpui_kit::{AssetSource, SharedString};
use std::borrow::Cow;
gpui_kit::assets::icon_assets!(
    CalendarIcons,
    [
        CalendarDays,
        Calendar,
        CalendarRange,
        ChevronLeft,
        ChevronRight,
        ChevronDown,
        Plus,
        X,
        Search,
        Settings,
        RefreshCw,
        Ellipsis,
        Clock,
        MapPin,
        Video,
        Link,
        Check,
        Square,
        SquareCheck,
        Trash,
        PencilLine,
        Copy,
        Repeat,
        Globe,
        ArrowLeft,
        PanelLeft,
        List,
        Download,
        Upload,
        CalendarCheck,
        CircleQuestionMark,
        Layers
    ]
);
pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let bytes = match CalendarIcons.load(path)? {
            Some(v) => Some(v),
            None => gpui_kit::assets::Assets.load(path)?,
        };
        Ok(bytes.map(|v| {
            if !path.ends_with(".svg") {
                return v;
            }
            Cow::Owned(
                String::from_utf8_lossy(&v)
                    .replace("stroke-linecap=\"round\"", "stroke-linecap=\"square\"")
                    .replace("stroke-linejoin=\"round\"", "stroke-linejoin=\"miter\"")
                    .into_bytes(),
            )
        }))
    }
    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut p = gpui_kit::assets::Assets.list(path)?;
        p.extend(CalendarIcons.list(path)?);
        p.sort();
        p.dedup();
        Ok(p)
    }
}
