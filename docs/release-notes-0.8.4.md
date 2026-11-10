# Markits Windows v0.8.4

- Return to the screenshot list immediately when finishing editing, while saving changes in the background. Keep outdated history cards unavailable until saving finishes.
- Generate annotated thumbnails directly at preview resolution to reduce rendering and PNG encoding work.
- Replace the fixed 350 ms Windows capture delay with a desktop compositor synchronization.
- Read captured PNG dimensions from the header instead of decoding the image again.
- Prevent a completed background save from marking a newly captured or modified image as saved.
