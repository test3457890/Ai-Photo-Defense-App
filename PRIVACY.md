# Privacy policy

The application processes selected files locally. It has no accounts, subscriptions, telemetry SDK, advertising, analytics endpoint or image-upload feature. It does not sell data. The current application code does not make network requests. Native file dialogs and system services may communicate within your operating system.

The current desktop collection and log live in memory. Export writes new images and JSON sidecars containing hashes, quality measurements and operation details. The optional experimental CLI reports model identifiers/hashes and optimization settings. The current reports omit source absolute paths. Terminal errors and desktop logs can contain filenames and output paths; treat them as local personal data when sharing diagnostics.

Temporary output files use the tempfile library and are cleaned up on normal failure. Process crashes, filesystem behavior, swap, thumbnails, antivirus, backups or cloud-synced export folders can retain copies. No secure deletion guarantee is made. Originals are never deleted, modified or overwritten by export. Clearing the collection does not erase source or output files.

Re-encoding drops source EXIF/GPS, XMP and ICC. It does not anonymize visible faces, locations, license plates or other visual information. Future model downloads, persisted history, updates or third-party integrations require an explicit policy update and visible controls. Offline manual model installation is the only current model workflow.
