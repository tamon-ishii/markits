## 1. Model & Theme Extensions

- [x] 1.1 Add `Pink` variant to `SemanticStyle` and configure tokens in `Theme`, verifying with unit tests.
- [x] 1.2 Add `shadow: bool` to `Scene` and `shadow`, `outline` options to annotations, verifying JSON parsing with unit tests.
- [x] 1.3 Add `Pin`, `Bullseye`, and `Divider` variants to `Annotation` enum, verifying JSON deserialization with unit tests.

## 2. Layout Engine Updates

- [x] 2.1 Implement layout placement for `Pin` (pin head + tip + adjacent text pill), verifying coordinates with unit tests.
- [x] 2.2 Implement layout geometry for `Bullseye` and `Divider`, verifying coordinates with unit tests.

## 3. SVG Renderer Updates

- [x] 3.1 Implement white-outlined text rendering using `paint-order="stroke fill"`, verifying generated SVG markup with unit tests.
- [x] 3.2 Implement `Pin` tag shape rendering (circle + pointer tip and attached pill), verifying markup with unit tests.
- [x] 3.3 Implement `Bullseye` and `Divider` element rendering, verifying markup with unit tests.
- [x] 3.4 Conditionally apply `filter="url(#markits-shadow)"` based on resolved shadow flag, verifying markup with unit tests.

## 4. Integration & Showcase

- [x] 4.1 Create an integration test and showcase example matching the Skitch design parts (outlined text, pins, bullseye, divider), verifying SVG output.
