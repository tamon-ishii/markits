export type SemanticStyle = 'primary' | 'secondary' | 'warning' | 'danger' | 'info' | 'step';
export type PositionHint = 'top' | 'bottom' | 'left' | 'right' | 'auto';

export interface Canvas {
  width: number;
  height: number;
}

export interface TargetRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type Point2D = [number, number] | { x: number; y: number };

export interface BaseAnnotation {
  type: string;
  style?: SemanticStyle;
  shadow?: boolean;
  stroke_width?: number;
  has_text?: boolean;
}

export type ArrowTextPlacement = 'middle' | 'end';

export interface ArrowAnnotation extends BaseAnnotation {
  type: 'arrow';
  target?: [number, number, number, number];
  position?: PositionHint;
  start?: [number, number];
  end?: [number, number];
  step?: number | string;
  text?: string;
  text_position?: ArrowTextPlacement;
  text_placement?: ArrowTextPlacement;
  box?: boolean;
  outline?: boolean;
  font_family?: string;
  font_size?: number;
}

export interface BezierArrowAnnotation extends BaseAnnotation {
  type: 'bezier-arrow';
  start: [number, number];
  control: [number, number];
  end: [number, number];
  position?: PositionHint;
  text?: string;
  text_position?: ArrowTextPlacement;
  text_placement?: ArrowTextPlacement;
  box?: boolean;
  offset?: number;
  outline?: boolean;
  font_family?: string;
  font_size?: number;
}

export interface CalloutAnnotation extends BaseAnnotation {
  type: 'callout';
  target: [number, number, number, number];
  text: string;
  position?: PositionHint;
  outline?: boolean;
  font_family?: string;
  font_size?: number;
}

export interface PinAnnotation extends BaseAnnotation {
  type: 'pin';
  target: [number, number, number, number];
  text?: string;
  icon?: string;
  position?: PositionHint;
  outline?: boolean;
  font_family?: string;
}

export interface BadgeAnnotation extends BaseAnnotation {
  type: 'badge';
  target: [number, number, number, number];
  step?: number | string;
  text?: string;
  position?: PositionHint;
  font_family?: string;
}

export interface StepArrowAnnotation extends BaseAnnotation {
  type: 'step-arrow';
  target: [number, number, number, number];
  step?: number | string;
  text?: string;
  position?: PositionHint;
  font_family?: string;
}

export interface LabelAnnotation extends BaseAnnotation {
  type: 'label';
  target: [number, number, number, number];
  text: string;
  position?: PositionHint;
  outline?: boolean;
  font_family?: string;
  font_size?: number;
}

export interface RectAnnotation extends BaseAnnotation {
  type: 'rect';
  target: [number, number, number, number];
}

export interface RoundedRectAnnotation extends BaseAnnotation {
  type: 'rounded-rect';
  target: [number, number, number, number];
  rx?: number;
  ry?: number;
}

export interface CircleAnnotation extends BaseAnnotation {
  type: 'circle';
  target: [number, number, number, number];
}

export interface BullseyeAnnotation extends BaseAnnotation {
  type: 'bullseye';
  target: [number, number, number, number];
}

export interface DividerAnnotation extends BaseAnnotation {
  type: 'divider';
  target: [number, number, number, number];
  position?: 'top' | 'bottom' | 'left' | 'right';
}

export interface SpotlightAnnotation extends BaseAnnotation {
  type: 'spotlight';
  target: [number, number, number, number];
}

export type Annotation =
  | ArrowAnnotation
  | BezierArrowAnnotation
  | CalloutAnnotation
  | PinAnnotation
  | BadgeAnnotation
  | StepArrowAnnotation
  | LabelAnnotation
  | RectAnnotation
  | RoundedRectAnnotation
  | CircleAnnotation
  | BullseyeAnnotation
  | DividerAnnotation
  | SpotlightAnnotation;

export interface Scene {
  canvas: Canvas;
  shadow?: boolean;
  annotations: Annotation[];
  hidden_annotations?: number[];
}

export interface DetectedUiElement {
  role: string;
  name?: string;
  window_id?: string;
  pid?: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface CapturedImage {
  width: number;
  height: number;
  data_url: string;
  ui_elements?: DetectedUiElement[];
}

export interface CropInfo {
  is_auto_cropped: boolean;
  offset_x: number;
  offset_y: number;
  base_width: number;
  base_height: number;
}

export interface LoadedImageResult {
  width: number;
  height: number;
  image_data_url: string;
  annotations_json: string | null;
  history_id?: string | null;
  ui_elements?: DetectedUiElement[] | null;
  base_image_data_url?: string | null;
  base_width?: number | null;
  base_height?: number | null;
  base_ui_elements?: DetectedUiElement[] | null;
  crop_info?: CropInfo | null;
}
