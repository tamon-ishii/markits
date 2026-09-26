use crate::model::SemanticStyle;

/// Visual styling tokens mapped from semantic styles.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleTokens {
    pub stroke_color: &'static str,
    pub fill_color: &'static str,
    pub text_color: &'static str,
    pub light_fill: &'static str,
    pub stroke_width: f64,
}

/// Design theme containing visual settings for all semantic styles.
#[derive(Debug, Clone)]
pub struct Theme {
    pub font_family: &'static str,
    pub font_size: f64,
    pub corner_radius: f64,
    pub spotlight_backdrop: &'static str,
    pub spotlight_opacity: f64,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            font_family: "system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif",
            font_size: 14.0,
            corner_radius: 6.0,
            spotlight_backdrop: "#000000",
            spotlight_opacity: 0.65,
        }
    }
}

impl Theme {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tokens_for(&self, style: SemanticStyle) -> StyleTokens {
        match style {
            SemanticStyle::Primary => StyleTokens {
                stroke_color: "#2563eb", // blue-600
                fill_color: "#2563eb",
                text_color: "#ffffff",
                light_fill: "rgba(37, 99, 235, 0.15)",
                stroke_width: 2.5,
            },
            SemanticStyle::Secondary => StyleTokens {
                stroke_color: "#4b5563", // gray-600
                fill_color: "#4b5563",
                text_color: "#ffffff",
                light_fill: "rgba(75, 85, 99, 0.15)",
                stroke_width: 2.5,
            },
            SemanticStyle::Warning => StyleTokens {
                stroke_color: "#d97706", // amber-600
                fill_color: "#d97706",
                text_color: "#ffffff",
                light_fill: "rgba(217, 119, 6, 0.15)",
                stroke_width: 2.5,
            },
            SemanticStyle::Danger => StyleTokens {
                stroke_color: "#dc2626", // red-600
                fill_color: "#dc2626",
                text_color: "#ffffff",
                light_fill: "rgba(220, 38, 38, 0.15)",
                stroke_width: 2.5,
            },
            SemanticStyle::Info => StyleTokens {
                stroke_color: "#0284c7", // sky-600
                fill_color: "#0284c7",
                text_color: "#ffffff",
                light_fill: "rgba(2, 132, 199, 0.15)",
                stroke_width: 2.5,
            },
            SemanticStyle::Step => StyleTokens {
                stroke_color: "#7c3aed", // violet-600
                fill_color: "#7c3aed",
                text_color: "#ffffff",
                light_fill: "rgba(124, 58, 237, 0.15)",
                stroke_width: 2.5,
            },
            SemanticStyle::Pink => StyleTokens {
                stroke_color: "#ea1a65", // Skitch iconic magenta/pink
                fill_color: "#ea1a65",
                text_color: "#ffffff",
                light_fill: "rgba(234, 26, 101, 0.15)",
                stroke_width: 2.5,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_tokens_mapping() {
        let theme = Theme::default();
        let primary = theme.tokens_for(SemanticStyle::Primary);
        let warning = theme.tokens_for(SemanticStyle::Warning);
        let danger = theme.tokens_for(SemanticStyle::Danger);
        let pink = theme.tokens_for(SemanticStyle::Pink);

        assert_eq!(primary.stroke_color, "#2563eb");
        assert_eq!(warning.stroke_color, "#d97706");
        assert_eq!(danger.stroke_color, "#dc2626");
        assert_eq!(pink.stroke_color, "#ea1a65");
        assert_eq!(pink.text_color, "#ffffff");
    }
}
