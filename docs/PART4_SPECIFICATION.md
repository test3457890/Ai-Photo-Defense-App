<!-- The owner's Part 4 instructions, saved verbatim as required by section 38. Implementation status: docs/IMPLEMENTATION_MATRIX.md#part-4. -->

AI Photo Defense: Part 4
Advanced Video, Audio and Multimedia AI Protection

1. GitHub repository
https://github.com/test3457890/Ai-Photo-Defense-App
2. Project scope
Continue developing the existing AI Photo Defense application after completing Parts 1, 2 and 3.
All previous Rust, cybersecurity, privacy, graphical interface and quality assurance requirements remain active.
Part 4 replaces the previously proposed professional video editing platform.
Do not develop a competing general-purpose video editor such as Adobe Premiere Pro, Final Cut Pro, Filmora or DaVinci Resolve.
Instead, develop a comprehensive multimedia protection platform that allows users to import finished media, apply adversarial AI protection and export protected versions while preserving the original visual and audio experience as closely as possible.
Support:

* Photographs
* Digital artwork
* Videos
* Audio recordings
* Podcasts
* Music
* Voice recordings
* Screen recordings
* Interviews
* Presentations containing audio or video
* Animated images

The application should become a final privacy and protection step before users publish their content online.
3. Primary objectives
Develop and evaluate protection against:

* Unauthorized AI image manipulation
* Unauthorized AI video manipulation
* Deepfake generation
* Facial identity cloning
* Face swapping
* Voice cloning
* Voice conversion
* AI-generated impersonation
* Unauthorized AI lip-sync manipulation
* Unauthorized AI training
* Video-to-video generation
* Image-to-video generation
* Speech synthesis based on unauthorized recordings
* Music imitation and unauthorized model training where technically applicable

Preserve:

* Original image appearance
* Original video appearance
* Original audio quality
* Natural speech
* Music quality
* Original colors
* Original resolution where possible
* Original frame rate
* Original audio channels
* Audio-video synchronization
* Natural movement
* Dynamic range

Do not claim that any protection method guarantees complete immunity against AI systems.
4. Mandatory Rust architecture
Maintain a Rust-native desktop application.
Implement production components in Rust, including:

* Multimedia processing controller
* Video protection engine
* Audio protection engine
* Media format detection
* Media decoding integration
* Media encoding integration
* Temporal protection engine
* Audio perturbation engine
* Video perturbation engine
* Protection configuration
* Privacy scanning
* Quality analysis
* Protection benchmarking
* Batch processing
* Secure export
* Model management
* Logging
* Diagnostics
* Application interface
* Command-line interface
* Developer SDK

Do not introduce:

* Electron
* React
* JavaScript-based desktop interfaces
* Node.js production runtimes
* Python-based production applications
* HTML and CSS desktop wrappers

Python may be used during development for research, model training, model conversion and comparative testing.
Necessary native multimedia libraries and machine learning runtimes are permitted when justified and securely packaged.
End users must not be required to install programming environments.
5. Supported operating systems
Maintain compatibility with:

* Windows 10, subject to documented security limitations
* Windows 11
* macOS Intel
* macOS Apple Silicon
* Linux x86-64
* Linux ARM64 where feasible
* Windows ARM64 where feasible

Support:

* CPU-only processing
* Optional GPU acceleration
* Automatic hardware detection
* Configurable memory limits
* Configurable worker counts
* Graceful hardware fallback
* Offline processing
* Local model execution

Investigate CUDA, Vulkan, Metal, DirectX 12 and supported hardware media acceleration.
6. Supported image formats
Preserve and expand image compatibility from previous development phases.
Standard image formats

* JPEG
* JPG
* PNG
* WebP
* TIFF
* TIF
* BMP
* AVIF
* HEIC
* HEIF
* JPEG XL where feasible
* JPEG 2000 where feasible

Animated image formats

* GIF
* APNG
* Animated WebP
* Animated AVIF where feasible

Professional photography formats

* DNG
* CR2
* CR3
* NEF
* ARW
* RAF
* ORF
* RW2
* PEF
* OpenEXR where appropriate

Additional formats to investigate

* SVG
* PSD
* XCF

Vector, layered and RAW formats may require conversion into protected raster exports. Preserve the original source file.
Do not claim that converting an image into a protected raster file also protects the original editable source.
7. Supported video formats
Implement compatibility with common video containers.
Video containers

* MP4
* MOV
* MKV
* WebM
* AVI
* M4V
* MPEG
* MPG
* MPEG-TS
* MTS
* M2TS
* MXF
* FLV
* WMV
* 3GP
* OGV

Video codecs

* H.264 / AVC
* H.265 / HEVC
* AV1
* VP8
* VP9
* MPEG-2
* MPEG-4 Part 2
* Apple ProRes
* DNxHD
* DNxHR
* Motion JPEG
* FFV1
* Theora

Investigate additional professional formats when technically feasible.
Support:

* Standard-definition video
* HD
* Full HD
* 2K
* 4K
* 6K
* 8K where hardware permits
* Vertical video
* Square video
* Variable frame rates
* High frame rates
* HDR
* SDR
* 8-bit video
* 10-bit video
* Higher bit depths where supported

Document actual decoding and encoding support separately.
Do not advertise formats that cannot be reliably protected and exported.
8. Supported audio formats
Implement standalone audio protection.
Uncompressed and lossless audio

* WAV
* FLAC
* AIFF
* AIF
* ALAC
* BWF
* CAF where feasible
* W64 where feasible

Compressed audio

* MP3
* AAC
* M4A
* Ogg Vorbis
* Opus
* WMA
* AC3
* E-AC3 where feasible
* AMR where feasible

Support:

* Mono
* Stereo
* Multi-channel audio
* Surround sound where supported
* Common audio sample rates
* High sample rates
* 16-bit audio
* 24-bit audio
* 32-bit floating-point audio where supported
* Variable bitrate
* Constant bitrate

Preserve audio channels and sample rates whenever possible.
Do not automatically convert stereo recordings to mono or reduce audio quality unnecessarily.
9. Automatic media format detection
The application must automatically inspect imported media.
Identify:

* Actual file format
* Container format
* Video codec
* Audio codec
* Resolution
* Frame rate
* Variable frame rate information
* Duration
* Bit depth
* Color space
* HDR metadata
* Audio sample rate
* Audio channel configuration
* Audio bitrate
* Video bitrate
* Subtitle tracks
* Embedded images
* Attached files
* Metadata
* Content credentials

Do not trust filename extensions alone.
Detect unsupported, corrupted or mislabeled files.
Offer compatible protection and export options based on the actual media format.
Maintain a format compatibility matrix specifying:

* Import support
* Preview support
* Protection support
* Export support
* Metadata inspection support
* CPU support
* GPU support
* Known limitations

10. FilmCraft multimedia engineering reference
Primary Rust multimedia reference:
https://github.com/storytold/filmcraft
Investigate FilmCraft's:

* Rust-native video codecs
* Audio codecs
* Video containers
* Media decoding
* Media encoding
* GPU processing
* CPU processing
* Exact media timing
* Audio-video synchronization
* Color management
* High-resolution media handling
* Audio processing
* Media export architecture
* Error handling
* Crash recovery

Reuse compatible components only after reviewing licensing, security, performance and technical suitability.
Do not integrate unnecessary timeline editing or professional video editing functionality.
FilmCraft should serve as a multimedia processing reference rather than define the scope of our application.
11. Additional multimedia repositories
Investigate all relevant repositories:
https://github.com/GStreamer/gstreamer-rs
https://github.com/FFmpeg/FFmpeg
https://github.com/itsakeyfut/avio
https://github.com/pdeljanov/Symphonia
https://github.com/RustAudio/cpal
https://github.com/RustAudio/rodio
https://github.com/gfx-rs/wgpu
https://github.com/xiph/rav1e
https://github.com/ralfbiedert/openh264-rs
https://github.com/mifi/lossless-cut
https://github.com/MediaArea/MediaInfoLib
Research additional Rust-native multimedia libraries and codecs.
Determine which multimedia architecture best fits the existing Rust application.
Avoid unnecessary duplication between media engines.
Verify source licenses, dependency licenses, patent obligations and redistribution permissions.
12. Voice cloning protection research
Investigate the following research projects and their strongest protection techniques.
AntiFake
https://github.com/WUSTL-CSPL/AntiFake
Investigate adversarial audio perturbations designed to interfere with unauthorized speech synthesis.
Voice Guard
https://github.com/LJY-M/Voice-Guard
Investigate minimally audible protection against voice conversion and related speech manipulation.
Enkidu
https://github.com/NoobCodeNameless/Enkidu
Investigate lightweight audio protection and potential real-time processing.
Good Noise
https://github.com/silsila-the-app/good-noise
Investigate audio privacy protection for voices and artistic recordings.
SceneGuard
https://github.com/richael-sang/SceneGuard
Investigate protection methods that account for the acoustic characteristics of a recording.
VoiceCloak
https://voice-cloak.github.io/VoiceCloak/
Investigate protection against diffusion-based voice cloning.
SafeSpeech
https://www.usenix.org/system/files/usenixsecurity25-zhang-zhisheng.pdf
Investigate adversarial speech protection and resistance to malicious speech synthesis.
E2E-VGuard
https://wxzyd123.github.io/e2e-vguard/
Investigate end-to-end voice synthesis protection.
CloneShield
https://arxiv.org/abs/2505.19119
Investigate reusable voice-cloning protection.
De-AntiFake
https://de-antifake.github.io/
Investigate documented weaknesses of existing voice protection methods and evaluate whether our protection remains effective after purification.
Locate official source repositories and current research publications where available.
Verify every project, license and technical claim before implementation.
Do not assume research results generalize to every commercial voice-cloning model.
13. Video AI protection research
Investigate:
DiffVax
https://diffvax.github.io/
Investigate efficient image and video immunization.
TC-UAP
https://saythe17.github.io/TC-UAP/
Investigate temporally consistent adversarial perturbations for video protection.
Anti-I2V
https://qualcomm-ai-research.github.io/anti-I2V/
Investigate protection against unauthorized image-to-video generation.
PhotoGuard
https://github.com/MadryLab/photoguard
FaceShield
https://github.com/kuai-lab/iccv25_faceshield
DiffusionGuard
https://github.com/choi403/DiffusionGuard
Continue investigating:

* FaceOff
* FaceLock
* SGD-Guard
* Diffusion Immunization
* Fawkes
* DCT-Shield
* BlurGuard
* VETO
* Glaze
* Nightshade

Research additional current video immunization, deepfake resistance and voice protection techniques.
Determine which methods are appropriate for photographs, individual video frames, complete video sequences and audio recordings.
14. Research integration requirements
For every research project, document:

* Official project name
* Repository
* Research publication
* Available source code
* Source license
* Model license
* Model availability
* Redistribution permissions
* Supported media types
* Protection objectives
* Known weaknesses
* Processing requirements
* Rust implementation feasibility
* CPU compatibility
* GPU compatibility
* Benchmark methodology
* Verified effectiveness

Combine complementary techniques through coordinated optimization.
Do not simply add random noise to images or recordings and claim AI protection.
Do not assume that combining multiple protective methods automatically improves effectiveness.
Test for interference between protection techniques.
Reimplement compatible research methods in Rust where practical.
15. Audio protection engine
Develop a local adversarial audio protection engine.
Investigate:

* Speaker embedding disruption
* Voice cloning resistance
* Voice conversion resistance
* Speech synthesis resistance
* Unauthorized speaker model training resistance
* Acoustic feature perturbation
* Frequency-domain protection
* Time-domain protection
* Psychoacoustic optimization
* Adaptive audio protection
* Multiple-model optimization
* Compression-resistant audio protection

Preserve:

* Speech intelligibility
* Voice characteristics as perceived by human listeners
* Natural pronunciation
* Accent
* Pitch
* Timing
* Music
* Background sounds
* Dynamic range

Avoid:

* Audible static
* Harsh distortion
* Unnecessary background noise
* Clipping
* Robotic sound
* Unintended pitch changes
* Unintended tempo changes

Protect voices through minimally audible changes wherever technically possible.
Provide a clear warning when achieving stronger protection requires noticeable audio changes.
16. Music and artist protection
Investigate protection for:

* Original music
* Vocal performances
* Instrumental recordings
* Singing voices
* Voice acting
* Audiobooks
* Podcasts
* Narration
* Live performances

Evaluate resistance to:

* Unauthorized voice cloning
* Unauthorized singing voice conversion
* Unauthorized music model training
* Unauthorized audio style imitation
* Audio-to-audio generation

Do not assume techniques designed for spoken language will work effectively on music.
Develop separate speech and music optimization profiles where supported by research and testing.
Preserve musical quality and stereo imaging.
17. Video protection engine
Develop a dedicated video immunization engine.
Investigate:

* Temporally consistent perturbations
* Facial identity protection
* Face-swapping resistance
* Deepfake resistance
* AI lip-sync manipulation resistance
* Video-to-video generation resistance
* Unauthorized video editing resistance
* Image-to-video generation resistance
* Frame extraction protection
* Motion-aware protection
* Scene-aware protection
* Compression-resistant perturbations
* Adaptive frame protection

Do not process every frame independently without evaluating temporal consistency.
Prevent:

* Visible flickering
* Color shifts
* Motion artifacts
* Frame instability
* Excessive noise
* Unnecessary image degradation

Preserve:

* Resolution
* Frame rate
* Motion
* Color accuracy
* Aspect ratio
* HDR information where supported
* Audio-video synchronization

Investigate optical flow, motion estimation and temporal optimization where appropriate.
18. Combined audiovisual protection
When a video contains audio, allow users to protect both streams.
Support:

* Video-only protection
* Audio-only protection
* Combined video and audio protection
* Selected audio track protection
* Selected video track protection
* Selected voice segment protection
* Selected face protection

Develop coordinated processing so audio and video protection do not introduce synchronization problems.
Handle videos containing:

* Multiple speakers
* Multiple faces
* Background music
* Sound effects
* Multiple audio tracks
* Subtitles
* Silent sections

Do not silently leave an unprotected alternate audio or video track inside a protected export.
19. Protection profiles
Develop media-specific protection profiles.
Original Quality
Prioritize preserving the original media experience while applying validated low-distortion protection.
Balanced Protection
Balance protection effectiveness, processing time and media quality.
Maximum Protection
Use stronger supported protection settings with clear quality and processing warnings.
Voice Protection
Focus on unauthorized voice cloning and speech synthesis.
Music Protection
Focus on musical recordings and artist voice protection.
Video Protection
Focus on visual manipulation and deepfake resistance.
Social Media Protection
Optimize protection for common platform compression and resizing.
Professional Protection
Provide advanced configuration for photographers, filmmakers, musicians and journalists.
Custom Protection
Allow individual algorithms, processing parameters and quality limits to be configured.
Profiles must represent real processing configurations.
Do not display fabricated protection percentages.
20. Social media compatibility
Investigate how media protection survives processing by common publishing platforms.
Include profiles for:

* YouTube
* YouTube Shorts
* Instagram
* Instagram Reels
* Instagram Stories
* Facebook
* TikTok
* Snapchat
* X
* LinkedIn
* Discord
* Reddit
* Vimeo
* Twitch
* SoundCloud
* Podcast platforms
* Personal websites

Investigate:

* Video compression
* Image compression
* Audio compression
* Resolution changes
* Frame rate conversion
* Cropping
* Bitrate reduction
* Audio normalization
* Metadata stripping
* Format conversion

Create reproducible local simulations of documented processing configurations.
Where authorized and feasible, test actual platform processing using synthetic or licensed media.
Do not claim a profile guarantees protection on a specific platform.
Record platform configurations and test dates because platform processing may change.
21. Quality preservation controls
Provide separate controls for:

* Image protection intensity
* Video protection intensity
* Audio protection intensity
* Visual fidelity
* Audio fidelity
* Processing time
* Compression resilience
* Memory usage
* Hardware acceleration

Allow users to establish quality limits.
If a protection configuration cannot meet the selected quality threshold, warn the user or stop processing according to their preferences.
Do not silently sacrifice quality.
22. Media comparison workspace
Develop an interface that allows users to compare:

* Original and protected photographs
* Original and protected video
* Original and protected audio

Video comparison
Include:

* Synchronized playback
* Side-by-side playback
* Split-screen comparison
* Fullscreen comparison
* Frame-by-frame inspection
* Zoom
* Playback speed adjustment
* Difference visualization

Audio comparison
Include:

* Original playback
* Protected playback
* Immediate A/B switching
* Waveform visualization
* Spectrogram visualization
* Frequency analysis
* Loudness measurements
* Difference signal playback where appropriate

Use loudness-matched comparisons so changes in volume do not mislead users about audio quality.
Clearly identify amplified differences as analytical visualizations rather than ordinary playback.
23. Multimedia Privacy Scan
Extend Part 2 Privacy Scan capabilities to video and audio.
Detect potentially sensitive information appearing in video, including:

* Faces
* People
* Computer screens
* Smartphones
* Documents
* Identification cards
* License plates
* Aircraft registration numbers
* Passwords
* API keys
* QR codes
* Barcodes
* Location information
* Reflections
* Whiteboards
* Sensitive background objects

Detect potentially sensitive information spoken in audio, including:

* Names
* Addresses
* Telephone numbers
* Passwords
* Access codes
* Personal identification numbers
* Financial information
* Private conversations
* Location information
* Sensitive workplace information

Provide optional privacy redaction.
Allow users to:

* Track and blur selected objects.
* Apply opaque video redaction.
* Mute selected audio segments.
* Bleep selected audio segments.
* Remove sensitive subtitle contents.
* Review flagged timestamps.

These are privacy tools, not a general-purpose multimedia editor.
24. Multimedia metadata removal
Inspect and optionally remove sensitive information from:

* EXIF
* XMP
* IPTC
* QuickTime metadata
* MP4 metadata
* Matroska metadata
* ID3 tags
* Vorbis comments
* BWF metadata
* GPS data
* Camera serial numbers
* Device identifiers
* Software identifiers
* Creation information
* Private comments
* Embedded thumbnails
* Cover images
* Hidden attachments
* Alternate streams

Preserve necessary playback information.
Allow users to retain approved artistic metadata, copyright information, album details and content credentials.
Do not expose private identities through optional authenticity signatures.
25. Watermarking and provenance
Develop optional:

* Invisible image watermarking
* Invisible video watermarking
* Audio watermarking
* Digital fingerprints
* Cryptographic hashes
* C2PA content credentials where supported
* Media authenticity verification

Research:
https://github.com/adobe/trustmark
https://github.com/contentauth/c2pa-rs
https://github.com/facebookresearch/audioseal
https://github.com/facebookresearch/videoseal
Evaluate additional relevant watermarking research.
Verify the licenses and model redistribution permissions.
Watermarking must remain optional.
Clearly distinguish watermarking from adversarial AI protection.
Watermarks may help establish provenance or identify copied material, but do not inherently prevent cloning.
26. Advanced Defense Lab
Expand Defense Lab to support multimedia protection testing.
Visual quality measurements

* SSIM
* PSNR
* VMAF
* Perceptual image similarity
* Color difference
* Temporal consistency
* Flicker measurements
* Frame distortion
* Motion consistency

Audio quality measurements

* Signal-to-noise ratio
* Speech intelligibility
* STOI
* ViSQOL
* Perceptual audio quality
* Spectral distortion
* Loudness consistency
* True peak
* Clipping detection
* Dynamic range
* Human listening evaluations

AI protection measurements

* Speaker embedding similarity
* Voice cloning similarity
* Speech synthesis resistance
* Voice conversion resistance
* Face-swapping resistance
* Facial identity similarity
* Video generation resistance
* AI manipulation resistance
* Cross-model transferability

Distinguish media quality measurements from actual protection effectiveness.
A high SSIM or VMAF score does not establish that AI cloning has been prevented.
Use consented identities and authorized testing media.
27. Compression resilience testing
Test protected media after:
Image transformations

* JPEG compression
* PNG conversion
* WebP compression
* AVIF compression
* Cropping
* Resizing
* Screenshots

Video transformations

* H.264 encoding
* H.265 encoding
* AV1 encoding
* VP9 encoding
* Bitrate reduction
* Resolution reduction
* Frame rate conversion
* Video cropping
* Frame extraction
* Screen recording
* Denoising
* Frame interpolation

Audio transformations

* MP3 encoding
* AAC encoding
* Opus encoding
* Sample rate conversion
* Bitrate reduction
* Loudness normalization
* Equalization
* Compression
* Noise reduction
* Filtering
* Re-recording where feasible

Evaluate whether protection remains effective after these transformations.
Investigate documented purification methods in a controlled research environment.
Record failures and limitations accurately.
28. Batch multimedia protection
Support:

* Multiple photographs
* Multiple audio files
* Multiple video files
* Mixed-media folders
* Automatic media classification
* Media-specific presets
* Recursive folder processing
* Processing queues
* Pause and resume
* Cancellation
* Error recovery
* Automatic resource allocation
* Configurable output directories
* Safe output names
* Format conversion
* Quality verification
* Protection reports

Allow users to apply different protection profiles to different media types within the same batch.
Never overwrite original files without explicit authorization.
29. Native desktop GUI requirements
Maintain the three existing interface modes.
Essential Mode
Provide a simple media protection utility.
Include:

* Drag-and-drop media import
* File list
* Automatic format identification
* Protection preset selection
* Progress indicators
* Basic quality settings
* Preview
* Export

Professional Mode
Provide:

* Large media preview
* Video playback
* Audio playback
* Synchronized comparisons
* Waveforms
* Spectrograms
* Separate audio and video protection controls
* Export configuration
* Privacy Scan
* Media information

Technical Mode
Provide:

* Defense Lab
* Model management
* Audio frequency analysis
* Video perturbation analysis
* Protection diagnostics
* Hardware monitoring
* Application logs
* Benchmarking
* Format inspection
* Detailed validation reports

Preserve the professional desktop design established in Part 3.
Do not create a generic AI-generated dashboard.
Do not create a complicated video editing timeline when a simple playback and protection workflow is sufficient.
30. Processing pipeline
Implement the following workflow:

1. Import the media file.
2. Identify its actual format.
3. Preserve the original file.
4. Validate the container and streams.
5. Inspect metadata.
6. Analyze visual and audio characteristics.
7. Perform optional Privacy Scan.
8. Apply approved privacy redactions.
9. Select compatible protection methods.
10. Apply image, video or audio protection.
11. Evaluate quality.
12. Optimize protection parameters.
13. Apply optional watermarking.
14. Remove selected private metadata.
15. Encode the protected output.
16. Apply optional authenticity credentials.
17. Reopen the exported media.
18. Verify media quality.
19. Verify audio-video synchronization.
20. Evaluate supported protection effectiveness.
21. Generate a sanitized verification report.

Test the final encoded output rather than relying exclusively on the uncompressed intermediate media.
31. Large-file processing
Support efficient processing of long videos and large audio recordings.
Investigate:

* Streaming decoding
* Chunk-based processing
* Overlapping processing windows
* Temporal boundary consistency
* Memory-mapped files where appropriate
* Tiled image processing
* GPU memory management
* Adaptive batch sizes
* Temporary storage management
* Checkpointing
* Interrupted processing recovery
* Background jobs

Test ordinary consumer hardware, including computers with 8 GB RAM.
Do not require the complete video to remain in memory.
Prevent discontinuities between independently processed chunks.
32. Shared logging and diagnostics
Reuse and expand the system-wide Rust logger developed in Part 3.
Record sanitized events related to:

* Media import failures
* Unsupported formats
* Decoder failures
* Encoder failures
* Corrupted files
* AI model failures
* Audio protection errors
* Video protection errors
* GPU failures
* Memory exhaustion
* Audio-video synchronization errors
* Quality threshold failures
* Export failures
* Recovery events
* Performance bottlenecks

Use stable error codes and processing job identifiers.
Never automatically log:

* Original media
* Private recordings
* Speech transcripts
* Detected credentials
* Facial embeddings
* Speaker embeddings
* Private file paths
* Sensitive metadata

Keep all diagnostic logs local by default.
Provide user-approved sanitized diagnostic reports.
33. Multimedia cybersecurity
Extend all existing security requirements.
Protect against:

* Malformed video containers
* Malicious codecs
* Crafted audio files
* Oversized image dimensions
* Excessive frame counts
* Invalid timestamps
* Invalid audio sample rates
* Resource exhaustion
* Integer overflows
* Malicious embedded metadata
* Malicious subtitle files
* Malicious attachments
* Malicious model files
* Path traversal
* Unsafe temporary files
* Unsafe symbolic links
* Unauthorized network access
* Supply chain vulnerabilities

Treat all media as untrusted input.
Use:

* Input validation
* Bounded allocations
* Checked arithmetic
* Resource limits
* Process isolation where appropriate
* Fuzz testing
* Secure file handling
* Dependency auditing
* Model integrity verification
* Signed application updates

Do not automatically execute or open URLs embedded in media.
34. Cross-platform testing
Verify:

* Windows builds
* macOS builds
* Linux builds
* CPU-only processing
* GPU processing
* Image protection
* Audio protection
* Video protection
* Combined audiovisual protection
* Format conversion
* Media playback
* Large-file processing
* Batch processing
* Exporting
* Quality verification
* Privacy scanning
* Logging
* Crash recovery

Distinguish successful compilation from actual runtime verification.
Test supported codecs and formats using licensed or synthetic media.
35. Competitive benchmarking
Compare against relevant research implementations.
Evaluate:

* AntiFake
* VoiceCloak
* SafeSpeech
* DiffVax
* PhotoGuard
* FaceShield
* Other relevant multimedia protection research

Measure:

* Protection effectiveness
* Audio quality
* Video quality
* Processing speed
* CPU utilization
* GPU utilization
* Memory usage
* Compression resilience
* Cross-model protection
* Ease of use

Use consistent test data and equivalent configurations wherever possible.
Do not claim superiority without supporting evidence.
36. Mandatory Rust checks
Run appropriate checks:
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo audit
cargo deny check
cargo build --workspace --release --locked
Include:

* Unit tests
* Integration tests
* Codec tests
* Audio processing tests
* Video processing tests
* Quality preservation tests
* Synchronization tests
* Security tests
* Fuzz tests
* Regression tests
* Performance benchmarks
* Logging privacy tests
* Cross-platform tests

Do not disable security checks to achieve passing builds.
37. Development phases
Part 4A: Multimedia foundation

* Audit Parts 1, 2 and 3.
* Evaluate FilmCraft.
* Establish multimedia decoding and encoding.
* Implement automatic format detection.
* Implement media preview.
* Establish format compatibility testing.

Part 4B: Audio protection

* Research voice protection algorithms.
* Implement Rust-compatible audio protection.
* Implement audio quality analysis.
* Test voice cloning resistance.
* Test audio compression resilience.

Part 4C: Video protection

* Research video immunization.
* Implement temporally consistent protection.
* Implement facial identity protection.
* Test deepfake resistance.
* Test video compression resilience.

Part 4D: Combined audiovisual protection

* Combine audio and video protection.
* Preserve audio-video synchronization.
* Implement media comparison tools.
* Implement standalone and combined protection profiles.

Part 4E: Multimedia privacy

* Extend Privacy Scan.
* Implement optional moving object redaction.
* Implement spoken information redaction.
* Extend metadata inspection.
* Implement secure export validation.

Part 4F: Production readiness

* Complete the GUI.
* Expand batch processing.
* Optimize performance.
* Complete security audits.
* Complete cross-platform testing.
* Document actual protection effectiveness.
* Publish verified releases.

38. GitHub requirements
Continue maintaining:

* README
* GitHub About description
* Architecture documentation
* Audio protection guide
* Video protection guide
* Format compatibility matrix
* Codec compatibility matrix
* Research register
* Licensing register
* Model compatibility matrix
* Hardware compatibility matrix
* Security policy
* Privacy policy
* Testing documentation
* Benchmark results
* Actual application screenshots
* Known limitations
* Development roadmap
* Changelog
* Development handoff

Save this Part 4 specification in the repository.
Keep the earlier general-purpose video editor proposal archived as a separate, deferred concept rather than an active requirement.
39. GitHub publication

* Inspect existing repository changes.
* Record the starting commit hash.
* Create appropriate development branches.
* Implement working functionality incrementally.
* Commit completed changes.
* Push to GitHub.
* Create focused pull requests.
* Review the actual implementation.
* Resolve security findings.
* Verify required checks.
* Merge validated pull requests.
* Verify the final main branch.
* Publish verified release artifacts when appropriate.

Do not merge insecure or failing code merely to mark the project complete.
40. Final development report
Provide:

1. Audit results for Parts 1 through 3.
2. Problems identified and corrected.
3. Rust compliance results.
4. FilmCraft integration findings.
5. Audio protection research findings.
6. Video protection research findings.
7. Supported image formats.
8. Supported video formats.
9. Supported audio formats.
10. Codec compatibility results.
11. Actual voice cloning resistance.
12. Actual video manipulation resistance.
13. Audio quality results.
14. Video quality results.
15. Compression resilience results.
16. Privacy verification results.
17. CPU performance.
18. GPU performance.
19. Memory consumption.
20. Cross-platform test results.
21. Security audit results.
22. Licensing findings.
23. Actual application screenshots.
24. Pull request links.
25. Commit hashes.
26. Merge status.
27. Release downloads.
28. Remaining limitations.
29. Blocked requirements.
30. Recommendations for independent review.
31. Final mandatory instructions
Take ownership of the development process.
Maintain a Rust-native, privacy-first application.
Do not develop an unnecessary general-purpose video editor.
Focus on protecting finished media before publication.
Do not introduce mandatory cloud processing.
Do not introduce hidden telemetry.
Do not upload private media.
Do not fabricate protection results.
Do not add meaningless random noise and claim AI resistance.
Do not claim that all AI cloning can be prevented.
Do not sacrifice original media quality without informing the user.
Do not remove existing functionality from Parts 1, 2 or 3.
Do not violate third-party licenses.
Do not present unavailable formats as supported.
Do not present simulated functionality as completed work.
Research, implement, test, inspect, repair, document, commit, push and merge validated functionality.
The ultimate objective is to develop a comprehensive Rust-native multimedia privacy application that makes unauthorized AI manipulation, facial cloning, voice cloning and training more difficult while preserving the original human viewing and listening experience as closely as possible.
