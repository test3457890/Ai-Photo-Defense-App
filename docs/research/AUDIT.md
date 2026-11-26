# Research and licensing audit

Checked 2026-11-26. This is an initial integration audit, not a completed legal review or comparative benchmark. GitHub API metadata and primary READMEs were inspected. License files were directly checked for PhotoGuard, FaceShield, FaceOff, DiffVax, Fawkes, BlurGuard, TrustMark, DeepSafe and both c2pa license alternatives. Other permissive entries below are provisional repository declarations until exact files and transitive components are reviewed. [Machine-readable snapshot](repository-snapshot.json) records observed repository license detection and push dates. A push date is not evidence that issues are supported or that a model still works.

No upstream defense code or model weights have been copied into this application. All listed effectiveness statements describe research scope or author claims. None have been reproduced by this project. An existing README, a successful optimization run or improved embedding distance does not prove real-world resistance.

## Identified sources

| Project and primary source | Scope to evaluate | Code/license finding | Models and redistribution decision |
|---|---|---|---|
| [PhotoGuard](https://github.com/MadryLab/photoguard) | Stable Diffusion encoder and diffusion optimization against editing/inpainting | Direct MIT license checked; old Python notebooks; last push 2023-02-27 | Diffusers/Stable Diffusion dependencies; separate weights terms and current availability need review. Candidate baseline, not bundled. |
| [FaceShield, ICCV 2025](https://github.com/kuai-lab/iccv25_faceshield) | Deepfake source-face identity features and attention | Direct LICENSE is KUAICV Research-Only, noncommercial. Checked blob 3646409821a3daf94bb2001b70794372813bcdfb | ArcFace and diffusion weights referenced, including serialized archives. Excluded from unrestricted distribution. Separate model licenses still apply. |
| [FaceOff](https://github.com/huzimun/FaceOff) | Tuning-free ID customization across image encoders | Direct Apache-2.0 license checked; dependency notices pending | PhotoMaker, IP-Adapter/Plus and SDXL identified in README. Weight redistribution unresolved. Research candidate only. |
| [FaceLock](https://github.com/taco-group/FaceLock) | Biometric identity destruction after generative edits | README MIT badge, but API did not identify a license file. Badge alone is insufficient | Face/aligner and generative model dependencies require detailed audit. Blocked for copied-code integration. |
| [SGD-Guard](https://github.com/BACKAI/SGD-Guard_IJCAI26) | Semantic gradient divergence and direction-sensitive EOT for face swapping | Paper's BACKAI/SGD-Guard URL redirects to this repository; license not resolved | README notes external models, unsafe serialized ArcFace modules and missing projection checkpoint. No direct model loading or redistribution approved. |
| [Diffusion Immunization / One Noise to Fool Them All](https://github.com/Parth-Badgujar/diffusion-immunization) | Universal and per-image latent objectives | MIT declaration; Python/PyTorch implementation | Stable Diffusion VAE dependencies. Separate weights/dataset review pending; no universal tensor redistributed. |
| [DiffusionGuard](https://github.com/choi403/DiffusionGuard) | Early diffusion objective and mask robustness | No detected repository license; readme.md inspected | Stable Diffusion inpainting weights referenced. Copied-code integration blocked until permission is resolved. |
| [DiffVax](https://github.com/ozdentarikcan/DiffVax) | Trained feed-forward immunizer, including video research | Direct MIT LICENSE checked, blob a253d697a9e963f00f69659e9246dbf7eb5bc894 | README lists checkpoints/diffvax_trained.pth and a dataset. Checkpoint provenance, safe conversion, base-model and dataset terms still need review. Strong candidate for evaluation, not shipped. |
| [Fawkes](https://github.com/Shawn-Shan/fawkes) | Cloaking facial recognition training inputs | Direct BSD-3-Clause-style license checked, blob 55d34e1e71e0c33ba47422b144950614dd0728c0; README also states intended personal/research use | Old TensorFlow stack and face/feature model terms need review. Recognition training protection is distinct from preventing generative face swaps. |
| [DCT-Shield paper](https://openaccess.thecvf.com/content/ICCV2025/papers/Bala_DCT-Shield_A_Robust_Frequency_Domain_Defense_against_Malicious_Image_Editing_ICCV_2025_paper.pdf) | DCT-domain optimization using a JPEG pipeline | Paper verified; an official code repository and code license were not established | VAE/model distribution and implementation audit unresolved. Do not substitute an unrelated repository. |
| [BlurGuard, NeurIPS 2025](https://github.com/jsu-kim/BlurGuard) | Adaptive regional noise smoothing to improve resistance to reversal | Direct MIT license checked; repository and primary conference abstract checked | Stable Diffusion and Segment Anything checkpoints referenced. Each needs license/safe-format review. Method must be tested jointly, not blindly appended. |
| [VETO](https://jonasgrebe.github.io/research/projects/veto/) | Reference/canvas joint-attention disruption in newer unified editors | Primary 2026 preprint project page found; no audited code license or definitive code repository established | Project links paper, demo and VetoBench. No redistribution approved. Include frontier-editor holdout tests before assuming older encoder defenses transfer. |
| [Glaze](https://glaze.cs.uchicago.edu/what-is-glaze.html) | Artist style mimicry protection | Official description says not open source; downloadable application has an [EULA](https://glaze.cs.uchicago.edu/software/Glaze-LICENSE.pdf) | No copied implementation or repackaging approved. Study public paper and use official baseline only under applicable terms. |
| [Nightshade](https://nightshade.cs.uchicago.edu/downloads.html) | Training-data poisoning, distinct from single-image editing defense | Official download is governed by its software license; unrestricted source/license not established in this audit | No binary or weights bundled. Separate from Glaze and from a claim that training becomes impossible. |
| [Adobe TrustMark](https://github.com/adobe/trustmark) | Invisible provenance watermarking and recovery | Direct MIT LICENSE checked, blob aa449b5deb115e74c2b86309791e64ce19b3be7a; README explicitly extends MIT to downloaded model files | Rust/ONNX implementation exists. Fetch exact model hashes and audit runtime before integration. Watermark presence does not stop edits or prove legal ownership. |
| [C2PA Rust](https://github.com/contentauth/c2pa-rs) | Signed content credentials and verification | Direct MIT and Apache-2.0 license files checked; dependencies have separate terms | No ML weights required. Signing certificate trust, key handling and privacy-safe manifests remain to design. Credentials can be removed and do not prevent editing. |
| [ShieldShot](https://github.com/varmabudharaju/ShieldShot) | Combined face perturbations, watermark and optional credentials | README says MIT, repository metadata did not resolve a license. Broad claims require independent scrutiny | README names generator.pt/encoder.pt/decoder.pt and admits unreliable watermark extraction. Weight download/provenance/license not verified. Not an approved dependency. |
| [DeepSafe](https://github.com/priyadarshini225/DeepSafe) | Integration of existing Python defenses and provenance ideas | Direct MIT license checked; README contains conflicting/duplicated status text and unsupported generalized percentages | Third-party rights are separate. No benchmark claims accepted and no bundled environments imported. Implementation and weight availability require a deeper audit. |

The name FaceShield also identifies unrelated anti-spoofing and differential-privacy repositories. FaceOff and BlurGuard have unrelated namesakes too. The links above identify the intended papers; names alone are not integration evidence.

## Additional research and counterexamples

- [LightShed, USENIX Security 2025](https://www.usenix.org/conference/usenixsecurity25/presentation/foerster): primary paper describes identification and removal of perturbation-based copyright protections. Include adaptive removal as an evaluation requirement. Its dataset results must not be generalized to every defense.
- [MetaCloak, CVPR 2024](https://metacloak.github.io/) and [official repository](https://github.com/liuyixin-louis/MetaCloak): subject-driven training protection through meta-learning and transformation robustness. Candidate training-defense baseline; license, weights and dataset rights remain unaudited here.
- [IMPRESS, NeurIPS 2023](https://papers.neurips.cc/paper_files/paper/2023/hash/222dda29587fbc2979ca99fd5ed00735-Abstract-Conference.html): reconstruction-consistency purification challenges perturbation defenses. Candidate adaptive attacker, not a user-facing removal tool in this project.
- VETO and DiffVax add 2026 research to the older baselines. Their publication recency does not establish effectiveness on today's closed APIs.
- [CLIP architecture/model card](https://huggingface.co/openai/clip-vit-base-patch32) and [original license](https://github.com/openai/CLIP/blob/main/LICENSE) support the experimental encoder implementation. The tested safetensors conversion is described separately in [the model experiment guide](../EXPERIMENTAL_CLIP.md). No pretrained model is approved for shipping yet.

## Integration gates

For each candidate, pin a commit; inspect the complete code license and all imported components; inventory required weights and preprocessing; verify origin and hashes; obtain explicit local-use and redistribution terms; avoid pickle/executable checkpoints; reproduce a clean baseline; then compare with equivalent images, models, budgets and transformations. Do not claim superiority from papers' incompatible metrics or budgets.

Publicly downloadable datasets containing real faces are not automatically licensed for this project. Use synthetic or consenting identities and rights-cleared photographs, with per-image provenance/consent records and subject-disjoint splits. Do not use the research repositories' celebrity portraits as the app's promotional screenshots.

## Additional license-file evidence

| Repository | File | Checked Git blob SHA |
|---|---|---|
| MadryLab/photoguard | LICENSE | d404feb3076864692beb0f9acc7ac23d420d872e |
| huzimun/FaceOff | LICENSE | 261eeb9e9f8b2b4b0d119366dda99c6fd7d35c64 |
| jsu-kim/BlurGuard | LICENSE | 451f54cdc030c9330c6291cd2f2a408915a4b54d |
| contentauth/c2pa-rs | LICENSE-MIT | e1e3bd7e9efec69f22be9f88fb8c37f6649a99d3 |
| contentauth/c2pa-rs | LICENSE-APACHE | 758d439916bc780a7fa5c90aebc94863527f904d |
| priyadarshini225/DeepSafe | LICENSE | fd3f147f75c4e5814917036efdbdbcd4e6a7c533 |

Root directory listings were also checked for FaceLock, SGD-Guard_IJCAI26, DiffusionGuard and ShieldShot. No license/COPYING/NOTICE candidate appeared there. This does not prove a license exists nowhere in the tree, but it confirms that a README badge or an unresolved API license field is insufficient integration approval.
