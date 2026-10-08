# Changelog

## [3.2.1](https://github.com/m1sk9/Pythia/compare/v3.2.0...v3.2.1) (2026-10-08)


### CI

* Retrigger the release workflow for v3.2.0 ([#344](https://github.com/m1sk9/Pythia/issues/344)) ([b21a3bb](https://github.com/m1sk9/Pythia/commit/b21a3bb4fac6fa8a41827fcebb2c15569dcc20ef))

## [3.2.0](https://github.com/m1sk9/Pythia/compare/v3.1.0...v3.2.0) (2026-10-08)


### Features

* Ignore replies to other users in conversation threads ([#337](https://github.com/m1sk9/Pythia/issues/337)) ([c3624ba](https://github.com/m1sk9/Pythia/commit/c3624baa82ee5620fe84b983b441501037b5e278))
* Show the model, version, or a custom text as Pythia's status ([#326](https://github.com/m1sk9/Pythia/issues/326)) ([a8c6714](https://github.com/m1sk9/Pythia/commit/a8c6714ef7b987e78a62a583df7eb94d4822a0e3))


### Bug Fixes

* Keep labels and attachment lines on truncation, start the context with a user turn, and rejoin split answers ([#334](https://github.com/m1sk9/Pythia/issues/334)) ([e6a003e](https://github.com/m1sk9/Pythia/commit/e6a003e522a947173d468d4dc5fde0e57443601a))


### Miscellaneous

* **deps:** update rust crate jiff to v0.2.38 ([#323](https://github.com/m1sk9/Pythia/issues/323)) ([cea1737](https://github.com/m1sk9/Pythia/commit/cea17373c68c14bcd964bad5a8143b6d9be06bb5))
* **deps:** update rust crate toml to v1.1.7 ([#325](https://github.com/m1sk9/Pythia/issues/325)) ([46d70b1](https://github.com/m1sk9/Pythia/commit/46d70b10686c28838d34e79fb7daf8e86f935c0b))

## [3.1.0](https://github.com/m1sk9/Pythia/compare/v3.0.0...v3.1.0) (2026-10-06)


### Features

* Let the model search and fetch the web through OpenRouter server tools ([#321](https://github.com/m1sk9/Pythia/issues/321)) ([e59613b](https://github.com/m1sk9/Pythia/commit/e59613bf7472ce625f5483d65ad7ae2fe352d110))

## [3.0.0](https://github.com/m1sk9/Pythia/compare/v2.4.1...v3.0.0) (2026-10-05)


### ⚠ BREAKING CHANGES

* The project is relicensed from MIT to Apache-2.0, Docker images move from ghcr.io/approvers/ichiyo_ai to ghcr.io/m1sk9/pythia, and v2 and earlier (ichiyoAI) are deprecated and no longer supported.

### Features

* Add an OpenRouter chat completions client with retries, typed errors, and model capability lookup (phase 2) ([#300](https://github.com/m1sk9/Pythia/issues/300)) ([a9be1fb](https://github.com/m1sk9/Pythia/commit/a9be1fbc9c2775eb8427830ea1889bc6dcb14b2c))
* Answer in threads with trigger detection, thread creation, single-flight turns, and response posting (phase 4) ([#303](https://github.com/m1sk9/Pythia/issues/303)) ([7aee1a6](https://github.com/m1sk9/Pythia/commit/7aee1a6afde3aceab8c7c6d14bf9f6f56d9d02d8))
* Build the conversation context from thread history (phase 3) ([#301](https://github.com/m1sk9/Pythia/issues/301)) ([5eee49c](https://github.com/m1sk9/Pythia/commit/5eee49cda06d808541d8518c1be2f364814d3b39))
* Pass image attachments to the model as Base64 (phase 5) ([#304](https://github.com/m1sk9/Pythia/issues/304)) ([eed39ac](https://github.com/m1sk9/Pythia/commit/eed39acde786d2fb0040c600ad4e6099756a9fcb))
* Ship a distroless Docker image, a Compose file, and v3 documentation (phase 7) ([#306](https://github.com/m1sk9/Pythia/issues/306)) ([4fb589c](https://github.com/m1sk9/Pythia/commit/4fb589c158950097c9edf6c88f899f3e68786e4d))


### Bug Fixes

* Split long answers at paragraph breaks before line breaks ([#307](https://github.com/m1sk9/Pythia/issues/307)) ([6ae3460](https://github.com/m1sk9/Pythia/commit/6ae34604cc49a454289fa3aed4ae3d4f91db054b))


### Miscellaneous

* change base renovate config ([#249](https://github.com/m1sk9/Pythia/issues/249)) ([4ea5146](https://github.com/m1sk9/Pythia/commit/4ea5146cf5b7a446d706c4cc4a3c1074b0b6be47))
* **deps:** bump h2 from 0.3.24 to 0.3.26 ([#221](https://github.com/m1sk9/Pythia/issues/221)) ([d3c2e61](https://github.com/m1sk9/Pythia/commit/d3c2e61aeeb17011e0a68f0710bccb9073f142bf))
* **deps:** bump openssl from 0.10.62 to 0.10.66 ([#258](https://github.com/m1sk9/Pythia/issues/258)) ([3e01388](https://github.com/m1sk9/Pythia/commit/3e01388d17e7a4a63a604df8d9c19f172f46d7a8))
* **deps:** bump openssl from 0.10.66 to 0.10.79 ([#265](https://github.com/m1sk9/Pythia/issues/265)) ([c65b55d](https://github.com/m1sk9/Pythia/commit/c65b55d04fd62607e65c72383b9870979f7563c8))
* **deps:** bump rustls from 0.21.6 to 0.21.11 ([#226](https://github.com/m1sk9/Pythia/issues/226)) ([a4f2efb](https://github.com/m1sk9/Pythia/commit/a4f2efb016b7148d6186faf0af489fccba80fc56))
* **deps:** bump tokio from 1.38.0 to 1.38.2 ([#267](https://github.com/m1sk9/Pythia/issues/267)) ([9bc403f](https://github.com/m1sk9/Pythia/commit/9bc403f69f97a80c0b66838def7915e930d9681a))
* **deps:** update docker/build-push-action action to v6 ([#251](https://github.com/m1sk9/Pythia/issues/251)) ([7fd09ca](https://github.com/m1sk9/Pythia/commit/7fd09ca7312360517da82518a9c9a3701fee4475))
* **deps:** update docker/setup-buildx-action action to v3.2.0 ([#217](https://github.com/m1sk9/Pythia/issues/217)) ([2a05ae0](https://github.com/m1sk9/Pythia/commit/2a05ae0d532604af07cbfe033b1519de011abae6))
* **deps:** update docker/setup-buildx-action action to v3.3.0 ([#224](https://github.com/m1sk9/Pythia/issues/224)) ([54d1dbe](https://github.com/m1sk9/Pythia/commit/54d1dbef93a60bf6f35d2772832a5a3b6810c7aa))
* **deps:** update peaceiris/actions-gh-pages action to v4 ([#252](https://github.com/m1sk9/Pythia/issues/252)) ([857b5b7](https://github.com/m1sk9/Pythia/commit/857b5b7d4663877d851dd0d1e72691caf4ff4c89))
* **deps:** update peaceiris/actions-mdbook action to v2 ([#253](https://github.com/m1sk9/Pythia/issues/253)) ([49c4207](https://github.com/m1sk9/Pythia/commit/49c4207fc0a5a59a1c17cebab65771ff4b66edd5))
* **deps:** update rust crate base64 to 0.22.1 ([#227](https://github.com/m1sk9/Pythia/issues/227)) ([1b08825](https://github.com/m1sk9/Pythia/commit/1b08825669ba9051c62a353c050a8e3cb546524e))
* **deps:** update rust crate dotenvy to 0.15.7 ([#228](https://github.com/m1sk9/Pythia/issues/228)) ([b84e814](https://github.com/m1sk9/Pythia/commit/b84e814e88a286f99831d04eb722df6808c5367b))
* **deps:** update rust crate envy to 0.4.2 ([#229](https://github.com/m1sk9/Pythia/issues/229)) ([973efba](https://github.com/m1sk9/Pythia/commit/973efba2d1f729e5ee0d05f5de58da934cc063ce))
* **deps:** update rust crate jiff to v0.2.37 ([#299](https://github.com/m1sk9/Pythia/issues/299)) ([db5b28c](https://github.com/m1sk9/Pythia/commit/db5b28c56ca93441a7c919af5afe392a14ecffb4))
* **deps:** update rust crate reqwest to 0.12 ([#218](https://github.com/m1sk9/Pythia/issues/218)) ([6a45071](https://github.com/m1sk9/Pythia/commit/6a45071a73e56f0e0481e75875c161b838ae369c))
* **deps:** update rust crate reqwest to 0.12.4 ([#230](https://github.com/m1sk9/Pythia/issues/230)) ([228dcca](https://github.com/m1sk9/Pythia/commit/228dcca20c11ea1316019e36012edaefaa933088))
* **deps:** update rust crate sentry to 0.32.3 ([#231](https://github.com/m1sk9/Pythia/issues/231)) ([ee7294d](https://github.com/m1sk9/Pythia/commit/ee7294d2b16003bb5e4b16cea643e01f5206bc91))
* **deps:** update rust crate sentry to 0.33.0 ([#243](https://github.com/m1sk9/Pythia/issues/243)) ([74b19f4](https://github.com/m1sk9/Pythia/commit/74b19f4b65ee36b119cd64257ea04b60608a0f33))
* **deps:** update rust crate sentry to 0.34.0 ([#246](https://github.com/m1sk9/Pythia/issues/246)) ([7c573d4](https://github.com/m1sk9/Pythia/commit/7c573d46a8e4980d7fdce6a36d8b3a17d9332df1))
* **deps:** update rust crate serde to 1.0.200 ([#232](https://github.com/m1sk9/Pythia/issues/232)) ([34a1e9b](https://github.com/m1sk9/Pythia/commit/34a1e9bbcc741f151e4be33bf913dde194dd3a5e))
* **deps:** update rust crate serde to v1.0.201 ([#239](https://github.com/m1sk9/Pythia/issues/239)) ([08e053d](https://github.com/m1sk9/Pythia/commit/08e053d326ec18225a18702314157e619f1b2e64))
* **deps:** update rust crate serde to v1.0.202 ([#241](https://github.com/m1sk9/Pythia/issues/241)) ([8cff169](https://github.com/m1sk9/Pythia/commit/8cff1690083a6729ec2f2fbc9675d86f9c33cb82))
* **deps:** update rust crate serde to v1.0.203 ([#242](https://github.com/m1sk9/Pythia/issues/242)) ([7491aab](https://github.com/m1sk9/Pythia/commit/7491aabc82ab1a2ed18d427e1a94c4582d80fa96))
* **deps:** update rust crate serde to v1.0.229 ([#283](https://github.com/m1sk9/Pythia/issues/283)) ([78b600f](https://github.com/m1sk9/Pythia/commit/78b600fbce719f2b887aadffedef57a1a009eb7c))
* **deps:** update rust crate serde_json to 1.0.116 ([#233](https://github.com/m1sk9/Pythia/issues/233)) ([732eca8](https://github.com/m1sk9/Pythia/commit/732eca89d14d7a8829baed766772830c9d3e7c48))
* **deps:** update rust crate serde_json to v1.0.117 ([#240](https://github.com/m1sk9/Pythia/issues/240)) ([c62b2c1](https://github.com/m1sk9/Pythia/commit/c62b2c14ead88cfe7931f58e195ddeb2067047b6))
* **deps:** update rust crate serde_json to v1.0.151 ([#254](https://github.com/m1sk9/Pythia/issues/254)) ([efd5611](https://github.com/m1sk9/Pythia/commit/efd56117a22b5559d530e180d96a35de84cde7f5))
* **deps:** update rust crate serenity to 0.12.1 ([#234](https://github.com/m1sk9/Pythia/issues/234)) ([bcad52b](https://github.com/m1sk9/Pythia/commit/bcad52b96c4f2a43ba05fd3eef1f08332537a754))
* **deps:** update rust crate serenity to v0.12.2 ([#245](https://github.com/m1sk9/Pythia/issues/245)) ([b9a45c9](https://github.com/m1sk9/Pythia/commit/b9a45c97b3d7a6261812de01f657cbed477b2b9b))
* **deps:** update rust crate tokio to 1.37 ([#220](https://github.com/m1sk9/Pythia/issues/220)) ([cb441b8](https://github.com/m1sk9/Pythia/commit/cb441b888ad759f823cc8bb583feb6a6da79fe20))
* **deps:** update rust crate tokio to v1.38.0 ([#244](https://github.com/m1sk9/Pythia/issues/244)) ([f540715](https://github.com/m1sk9/Pythia/commit/f5407151a55757c09c63c5cb21ef5c9bca9c0106))
* **deps:** update rust crate tokio to v1.53.2 ([#279](https://github.com/m1sk9/Pythia/issues/279)) ([a87b906](https://github.com/m1sk9/Pythia/commit/a87b90613258acb05763ab4fb17883d2ef5bfa77))
* **deps:** update rust crate tokio-stream to 0.1.15 ([#235](https://github.com/m1sk9/Pythia/issues/235)) ([a69d386](https://github.com/m1sk9/Pythia/commit/a69d3868296e3a5494c57d1d22fc11c493af6b11))
* **deps:** update rust crate tracing to 0.1.40 ([#236](https://github.com/m1sk9/Pythia/issues/236)) ([065698f](https://github.com/m1sk9/Pythia/commit/065698f388de1b37d55a9b4f8bf5537ad5b6f674))
* **deps:** update rust crate tracing-subscriber to 0.3.18 ([#237](https://github.com/m1sk9/Pythia/issues/237)) ([98cd6f6](https://github.com/m1sk9/Pythia/commit/98cd6f637d95d7d5bd7ceb87638befdc780a6210))
* **deps:** update rust docker tag to v1.77.0 ([#219](https://github.com/m1sk9/Pythia/issues/219)) ([f85574b](https://github.com/m1sk9/Pythia/commit/f85574bc0228c6b8c0946b3e6aef21628f50b94e))
* **deps:** update rust docker tag to v1.77.1 ([#222](https://github.com/m1sk9/Pythia/issues/222)) ([12b77a3](https://github.com/m1sk9/Pythia/commit/12b77a3970e6b840e95230988cd127833eadf640))
* **deps:** update rust docker tag to v1.77.2 ([#223](https://github.com/m1sk9/Pythia/issues/223)) ([b3a6516](https://github.com/m1sk9/Pythia/commit/b3a65162b49643e7cffc44e7e0a0ca57d89aa09a))
* **deps:** update rust docker tag to v1.78.0 ([#238](https://github.com/m1sk9/Pythia/issues/238)) ([a72368d](https://github.com/m1sk9/Pythia/commit/a72368de5d36eeb6a350b009cb7a3d803872c023))
* **deps:** update rust docker tag to v1.79.0 ([#247](https://github.com/m1sk9/Pythia/issues/247)) ([6209623](https://github.com/m1sk9/Pythia/commit/6209623d658270fc6e84d6bae3bb637e9d68998c))
* **deps:** update rust docker tag to v1.99.0 ([#259](https://github.com/m1sk9/Pythia/issues/259)) ([a3bc19f](https://github.com/m1sk9/Pythia/commit/a3bc19f8b97081445bc7415d4339f57c058036d9))
* **deps:** update taiki-e/install-action digest to e407f7b ([#308](https://github.com/m1sk9/Pythia/issues/308)) ([f025b10](https://github.com/m1sk9/Pythia/commit/f025b10e40cc8089ea46dce0cfe68320189bfe98))
* **deps:** update tokio-tracing monorepo ([#282](https://github.com/m1sk9/Pythia/issues/282)) ([5cd78a3](https://github.com/m1sk9/Pythia/commit/5cd78a34263b677cc8e1642aaaa494eccf1586a3))
* migrate maintenance from Approvers to m1sk9 and relicense under Apache-2.0 ([#287](https://github.com/m1sk9/Pythia/issues/287)) ([6b032e3](https://github.com/m1sk9/Pythia/commit/6b032e37dc47887bbcab47b658bbcb2d038dcb59))
* remove obsolete CODEOWNERS file ([#278](https://github.com/m1sk9/Pythia/issues/278)) ([0ec37b3](https://github.com/m1sk9/Pythia/commit/0ec37b39ca28070c2752e65a947654a5c2f50764))


### CI

* drop package name from release tags ([#289](https://github.com/m1sk9/Pythia/issues/289)) ([b45663b](https://github.com/m1sk9/Pythia/commit/b45663b02dda13f22821f4da646b493a17c0299f))
* Measure test coverage with Codecov ([#305](https://github.com/m1sk9/Pythia/issues/305)) ([e4639f9](https://github.com/m1sk9/Pythia/commit/e4639f9a19ca89fe4d4afe81dddaa94d59c44097))

## [2.4.1](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v2.4.0...ichiyo_ai-v2.4.1) (2024-03-06)


### Bug Fixes

* **deps:** bump mio from 0.8.9 to 0.8.11 ([#215](https://github.com/approvers/ichiyoAI/issues/215)) ([42d9987](https://github.com/approvers/ichiyoAI/commit/42d9987c0014e1dac56a42f5fd8e88eab8f314b8))

## [2.4.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v2.3.1...ichiyo_ai-v2.4.0) (2024-02-11)


### Features

* update model `gpt-3.5-turbo-0125` ([#204](https://github.com/approvers/ichiyoAI/issues/204)) ([176d95d](https://github.com/approvers/ichiyoAI/commit/176d95d979a2588af503a9db0bbdbde1076349c5))


### Bug Fixes

* **docker:** release-please config `package-name` ([#207](https://github.com/approvers/ichiyoAI/issues/207)) ([3bf04e9](https://github.com/approvers/ichiyoAI/commit/3bf04e98506055c68799692c3d1bc481487f985f))

## [2.3.1](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v2.3.0...ichiyo_ai-v2.3.1) (2024-01-31)


### Bug Fixes

* fix some typos and grammar errors ([#199](https://github.com/approvers/ichiyoAI/issues/199)) ([72b18c5](https://github.com/approvers/ichiyoAI/commit/72b18c550740b31ed7f66f156aeda7bea929967a))

## [2.3.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v2.2.0...ichiyo_ai-v2.3.0) (2024-01-27)


### Features

* GPT-4 Turbo `gpt-4-0125-preview` のサポート ([#195](https://github.com/approvers/ichiyoAI/issues/195)) ([87dc83e](https://github.com/approvers/ichiyoAI/commit/87dc83e2f0e5b9a3428cd87e3666faeec52c9c31))

## [2.2.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v2.1.1...ichiyo_ai-v2.2.0) (2024-01-21)


### Features

* **bin:** DALL-E, ChatGPT のリクエストタイムアウトを緩和 ([#193](https://github.com/approvers/ichiyoAI/issues/193)) ([3486e64](https://github.com/approvers/ichiyoAI/commit/3486e649fc8eaa9bc084ecb1a8990816e612af3f))

## [2.1.1](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v2.1.0...ichiyo_ai-v2.1.1) (2024-01-19)


### Bug Fixes

* **deps:** bump h2 from 0.3.21 to 0.3.24 ([#191](https://github.com/approvers/ichiyoAI/issues/191)) ([6714886](https://github.com/approvers/ichiyoAI/commit/671488608064a19693d5e0cf19ef0e80901aa4be))

## [2.1.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v2.0.0...ichiyo_ai-v2.1.0) (2024-01-11)


### Features

* **lib:** Google AI 関係のエラー報告機能を強化 ([#189](https://github.com/approvers/ichiyoAI/issues/189)) ([791addf](https://github.com/approvers/ichiyoAI/commit/791addfcf1744944bfdda53a903139cad8cb4dfd))

## [2.0.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.21.0...ichiyo_ai-v2.0.0) (2024-01-08)


### ⚠ BREAKING CHANGES

ichiyoAI v2.0.0 では Gemini への対応等や Application Command への対応が行われました.

* 環境変数に `GOOGLE_AI_API_KEY`, `SENTRY_DSN` が追加されました.
  * `GOOGLE_AI_API_KEY` は指定しないと起動できません. [Google AI Studio](https://makersuite.google.com) で発行できます.
  * `SENTRY_DSN` は指定しなかった場合 Sentry によるエラー監視が行われません.
* ichiyoAI は起動時にギルドコマンドで Application Command を設定します.
  * 自分の環境で起動する場合は Bot に登録されたコマンドが上書きされても大丈夫か確認してからにしてください.
  * 限界開発鯖版はそのまま利用できます.

### Features

* v2 の実装 ([#185](https://github.com/approvers/ichiyoAI/issues/185)) ([4157da0](https://github.com/approvers/ichiyoAI/commit/4157da06c415caca2bb89b4c0aa91a37f84cc5e1))
* Text Generation が Message Command で利用できるようになりました.
  * これにより, 限界税に納税しているメンバーでも GPT-3.5 Turbo が利用できます.
  * メンションによる生成は廃止されました. 新しい利用方法については [ドキュメント](https://ichiyoai.approvers.dev/how-to/text-generation.html) を参照してください.
  * **重要:** 限界税を納税していない場合は `Text (GPT-4 Turbo)` コマンドは表示されません. (限界開発鯖版のみ)
* Image Generation が Application Command で利用できるようになりました.
  * これにより, 限界税に納税しているメンバーでも DALL-E 2 が利用できます.
  * `!image` による生成は廃止されました. 新しい利用方法については [ドキュメント](https://ichiyoai.approvers.dev/how-to/image-generation.html) を参照してください.
* Gemini Pro が利用できるようになりました.
  * Google が開発した GPT-3.5 Turbo を凌駕するマルチモーダル大規模言語モデルです.
  * Gemini Pro は GPT-3.5 Turbo のライバルとして位置づけられており, DeepMind Technologies によるベンチマークでは多くの測定において GPT-3.5 Turbo を上回っています.
  * 詳しいベンチマーク結果は [こちら](https://deepmind.google/technologies/gemini/#capabilities)

## [1.21.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.20.1...ichiyo_ai-v1.21.0) (2024-01-03)


### Features

* Sentry の有効化 ([#182](https://github.com/approvers/ichiyoAI/issues/182)) ([3b647f3](https://github.com/approvers/ichiyoAI/commit/3b647f32c7402f609a3adee1b472b643b69ad223))

## [1.20.1](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.20.0...ichiyo_ai-v1.20.1) (2024-01-02)


### Bug Fixes

* `serde_json` の "expected a borrowd string" のエラー ([#179](https://github.com/approvers/ichiyoAI/issues/179)) ([5cc69ce](https://github.com/approvers/ichiyoAI/commit/5cc69ce5496617e5b3200080242e597a067d262a))

## [1.20.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.19.1...ichiyo_ai-v1.20.0) (2024-01-02)


 [!NOTE]
 ichiyoAI v1 最後のリリースになります

### Features

* 内部で使う生成系 AI の API の wrapper を実装する ([#165](https://github.com/approvers/ichiyoAI/issues/165)) ([794c59f](https://github.com/approvers/ichiyoAI/commit/794c59f934f83f10ccab786b38b81c16fabac56e))
* 内部で使う画像生成系の AI の API の wrapper を実装する ([#177](https://github.com/approvers/ichiyoAI/issues/177)) ([5775763](https://github.com/approvers/ichiyoAI/commit/577576383f63f5bc5e594492a3e6eb432d71705d))
  * Text Generation, Image Generation は ichiyoAI:lib (内部Wrapper) に置き換わりました.
  * これにより, `async_openai` に依存しなくなりました.
  * 2つの変更は [@Nanai10a](https://github.com/Nanai10a) が実装しました. Thanks :heart:

## [1.19.1](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.19.0...ichiyo_ai-v1.19.1) (2023-12-09)


### Bug Fixes

* メンションを正しく取り除かれない不具合の修正 ([#158](https://github.com/approvers/ichiyoAI/issues/158)) ([63b844c](https://github.com/approvers/ichiyoAI/commit/63b844c88627200192275fbd730d8d7da001ac35))

## [1.19.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.18.0...ichiyo_ai-v1.19.0) (2023-12-02)


### Features

* support serenity v0.12 ([#155](https://github.com/approvers/ichiyoAI/issues/155)) ([257c375](https://github.com/approvers/ichiyoAI/commit/257c375e2059cfff44638825a5a540c7426b6cd4))

## [1.18.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.17.0...ichiyo_ai-v1.18.0) (2023-11-26)


### Features

* 埋め込みの DALL-E のモデル名表示 ([#152](https://github.com/approvers/ichiyoAI/issues/152)) ([08bfe87](https://github.com/approvers/ichiyoAI/commit/08bfe87ab9e17b7cfc8b27b8f58175fb3e7ce985))

## [1.17.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.16.0...ichiyo_ai-v1.17.0) (2023-11-25)


### Features

* async_openai v0.17.0 のサポート ([#148](https://github.com/approvers/ichiyoAI/issues/148)) ([b1b0b09](https://github.com/approvers/ichiyoAI/commit/b1b0b090993c3b99c0f7ddc4bb9b6a90d8e82f51))
* ImageGeneration 機能のサポート ([#151](https://github.com/approvers/ichiyoAI/issues/151)) ([5f88fb9](https://github.com/approvers/ichiyoAI/commit/5f88fb9ef15ae26cfda3464d9aabfb478d2cd857))

## [1.16.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.15.4...ichiyo_ai-v1.16.0) (2023-11-07)


### Features

* GPT-4 Turbo (gpt-4-1106-preview) のサポート ([#142](https://github.com/approvers/ichiyoAI/issues/142)) ([0cc7f5e](https://github.com/approvers/ichiyoAI/commit/0cc7f5ea23b5962cf01f75d179fb8ff71ad3dded))
* Updated GPT 3.5 Turbo (gpt-3.5-turbo-1106) のサポート ([#143](https://github.com/approvers/ichiyoAI/issues/143)) ([40c5300](https://github.com/approvers/ichiyoAI/commit/40c53000e4c378ea935cfab229042cd44af88083))
* デバックログの切り替えロジックを作成 ([#140](https://github.com/approvers/ichiyoAI/issues/140)) ([cfdecd1](https://github.com/approvers/ichiyoAI/commit/cfdecd19fdfaa226cc4ee563be081bcbc5afbc0f))
* メッセージフォーマットの改善 ([#144](https://github.com/approvers/ichiyoAI/issues/144)) ([8567760](https://github.com/approvers/ichiyoAI/commit/8567760efb44496cd4b9ac2094cd511f56e8f797))

## [1.15.4](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.15.3...ichiyo_ai-v1.15.4) (2023-10-28)


### Bug Fixes

* コンテキストが正しく維持されない問題を修正 ([#135](https://github.com/approvers/ichiyoAI/issues/135)) ([98c1a62](https://github.com/approvers/ichiyoAI/commit/98c1a62f8169ce1e79fa6b2d59f72bc541963185))

## [1.15.3](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.15.2...ichiyo_ai-v1.15.3) (2023-10-28)


### Bug Fixes

* 使用するトークン長が短くレスポンスが不完全になる問題の修正 ([#133](https://github.com/approvers/ichiyoAI/issues/133)) ([72c134f](https://github.com/approvers/ichiyoAI/commit/72c134f8979c3a4ac0ce8a5e17b302143f58d9a8))

## [1.15.2](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.15.1...ichiyo_ai-v1.15.2) (2023-10-28)


### Bug Fixes

* Docker Container 上で動作しない問題の修正 ([#131](https://github.com/approvers/ichiyoAI/issues/131)) ([458db6c](https://github.com/approvers/ichiyoAI/commit/458db6c3b2dd0d1282942b7d29ef7392c7615833))

## [1.15.1](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.15.0...ichiyo_ai-v1.15.1) (2023-10-28)


### Bug Fixes

* バージョン取得の方法を修正 ([#129](https://github.com/approvers/ichiyoAI/issues/129)) ([1531a4d](https://github.com/approvers/ichiyoAI/commit/1531a4dd0fb0d8d26a3e29582b808aa1786cc56b))

## [1.15.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.14.0...ichiyo_ai-v1.15.0) (2023-10-25)


### Features

* [rollback] release v1.13.0 (v1.14.0) ([#126](https://github.com/approvers/ichiyoAI/issues/126)) ([7c824ce](https://github.com/approvers/ichiyoAI/commit/7c824ce8bceee3a7ba662b2ffd01bde2f8b35562))
* v1.15.0 の強制リリース ([#128](https://github.com/approvers/ichiyoAI/issues/128)) ([9f0003d](https://github.com/approvers/ichiyoAI/commit/9f0003de4cf799f940042e04a58394762be39bc2))

## [1.14.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.13.0...ichiyo_ai-v1.14.0) (2023-10-23)


### Features

* ichiyoAI v1.13.0 ([#120](https://github.com/approvers/ichiyoAI/issues/120)) ([6c714a3](https://github.com/approvers/ichiyoAI/commit/6c714a3c08afa47c688a5927419e3a73b855b389))

## [1.13.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.12.2...ichiyo_ai-v1.13.0) (2023-10-23)


### Features

* recreate ChatGPT logic ([#122](https://github.com/approvers/ichiyoAI/issues/122)) ([699810b](https://github.com/approvers/ichiyoAI/commit/699810b0219b8febaee0063cba4ad982d4eca750))

## [1.12.2](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.12.1...ichiyo_ai-v1.12.2) (2023-10-03)


### Bug Fixes

* 返信モードのコンテキストが5文字以上なのにエラーになる問題の修正 ([#112](https://github.com/approvers/ichiyoAI/issues/112)) ([152613e](https://github.com/approvers/ichiyoAI/commit/152613e6555b2fa86df16e3c8deaac0e0bcbaefb))

## [1.12.1](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.12.0...ichiyo_ai-v1.12.1) (2023-09-24)


### Bug Fixes

* Docker Image のビルドに失敗する問題の修正 ([#109](https://github.com/approvers/ichiyoAI/issues/109)) ([5dd8a39](https://github.com/approvers/ichiyoAI/commit/5dd8a3917762330bddf9d3b7526f1f635a247bdb))

## [1.12.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.11.1...ichiyo_ai-v1.12.0) (2023-09-23)


### Features

* 5文字以上コンテキストがない会話は強制的に終了するように ([#106](https://github.com/approvers/ichiyoAI/issues/106)) ([e215af3](https://github.com/approvers/ichiyoAI/commit/e215af3f551a51670d71d1cdafca4a4ef66cf0f2))

## [1.11.1](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.11.0...ichiyo_ai-v1.11.1) (2023-09-17)


### Bug Fixes

* 利用料金表示の誤字を修正 ([#103](https://github.com/approvers/ichiyoAI/issues/103)) ([8c7964b](https://github.com/approvers/ichiyoAI/commit/8c7964bc2b175bdf43537c8a94264854ee1a3825))

## [1.11.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.10.0...ichiyo_ai-v1.11.0) (2023-09-16)


### Features

* 応答メッセージに合計トークン数を表示するように ([#102](https://github.com/approvers/ichiyoAI/issues/102)) ([19136a4](https://github.com/approvers/ichiyoAI/commit/19136a4a434c05d337c541b465926f54b9924d2d))


### Bug Fixes

* エラーメッセージがメンションされず送信される問題の修正 ([#100](https://github.com/approvers/ichiyoAI/issues/100)) ([e472107](https://github.com/approvers/ichiyoAI/commit/e472107043f2b6a79b190fc6ce8cdc0f21ab56ad))

## [1.10.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.9.2...ichiyo_ai-v1.10.0) (2023-08-27)


### Features

* 使用モデルの表示を追加 ([#89](https://github.com/approvers/ichiyoAI/issues/89)) ([96f8805](https://github.com/approvers/ichiyoAI/commit/96f8805c276ab3c83b631b3ac2dd463f000f1ab8))

## [1.9.2](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.9.1...ichiyo_ai-v1.9.2) (2023-08-26)


### Bug Fixes

* コンテキストが途中で欠如する問題の修正 ([#85](https://github.com/approvers/ichiyoAI/issues/85)) ([2fab02e](https://github.com/approvers/ichiyoAI/commit/2fab02e7897a3dc2c28f701f48faa1dd6c28c2ff))

## [1.9.1](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.9.0...ichiyo_ai-v1.9.1) (2023-08-26)


### Bug Fixes

* Docker コンテナ上のSSL認証エラーを修正 ([#83](https://github.com/approvers/ichiyoAI/issues/83)) ([f09d97d](https://github.com/approvers/ichiyoAI/commit/f09d97d2a9d64b8f47d4775598289cc233eeef17))

## [1.9.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.8.0...ichiyo_ai-v1.9.0) (2023-08-24)


### Features

* sentry のセットアップ ([#81](https://github.com/approvers/ichiyoAI/issues/81)) ([011c1bd](https://github.com/approvers/ichiyoAI/commit/011c1bd91e5b4caa5dc4472d4132510f83a01218))

## [1.8.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.7.0...ichiyo_ai-v1.8.0) (2023-08-20)


### Features

* レスポンスメッセージに利用料金の表示を追加 ([#72](https://github.com/approvers/ichiyoAI/issues/72)) ([2d2c0fc](https://github.com/approvers/ichiyoAI/commit/2d2c0fc5a81d6bf86ba794ae2c74f133df357c18))

## [1.7.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.6.0...ichiyo_ai-v1.7.0) (2023-08-17)


### Features

* 2000文字を超えないようにシステムコンテキストを設定する ([#65](https://github.com/approvers/ichiyoAI/issues/65)) ([e892315](https://github.com/approvers/ichiyoAI/commit/e892315991150d25fafcfc02c91415cfbcc5398d))

## [1.6.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.5.0...ichiyo_ai-v1.6.0) (2023-08-15)


### Features

* タイムアウト秒数を緩和 ([#63](https://github.com/approvers/ichiyoAI/issues/63)) ([225adc2](https://github.com/approvers/ichiyoAI/commit/225adc253e60e236a2ad5908b12bdeb47f0d1da6))

## [1.5.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.4.0...ichiyo_ai-v1.5.0) (2023-08-14)


### Features

* サブスクライバー限定で GPT-4 を解放 ([#59](https://github.com/approvers/ichiyoAI/issues/59)) ([f407fd7](https://github.com/approvers/ichiyoAI/commit/f407fd7c45ad38bed82e91553537b66badc226c0))


### Bug Fixes

* 返信時メンションしない不具合を修正 ([#61](https://github.com/approvers/ichiyoAI/issues/61)) ([92db0b1](https://github.com/approvers/ichiyoAI/commit/92db0b123cab54aef19acc15c87d3eaa946bd298))

## [1.4.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.3.0...ichiyo_ai-v1.4.0) (2023-08-14)


### Features

* アクティビティ欄にバージョンを表示するように ([#58](https://github.com/approvers/ichiyoAI/issues/58)) ([f81d055](https://github.com/approvers/ichiyoAI/commit/f81d0555a600eadfb67d63a4ed4a33b71084252e))


### Bug Fixes

* **ci:** higuchi-ichiyo の制御用 AppID を指定する ([#55](https://github.com/approvers/ichiyoAI/issues/55)) ([7a49d4d](https://github.com/approvers/ichiyoAI/commit/7a49d4d50ecef7358cf712ae11dd5efe3d8cb5c5))

## [1.3.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.2.0...ichiyo_ai-v1.3.0) (2023-08-13)


### Features

* 返信のコンテキストを維持するように変更 ([#49](https://github.com/approvers/ichiyoAI/issues/49)) ([d97a047](https://github.com/approvers/ichiyoAI/commit/d97a04711bc24c3071d05fa1c4db797c48ac4762))

## [1.2.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.1.0...ichiyo_ai-v1.2.0) (2023-08-12)


### Features

* mitigation openai timeout ([a0cc9bc](https://github.com/approvers/ichiyoAI/commit/a0cc9bcf4a0ca766e6653d335e7ab2532120c29c))
* support sender name ([4a7e7e1](https://github.com/approvers/ichiyoAI/commit/4a7e7e110b6eea2269b328dd06ed1c00502224c0))

## [1.1.0](https://github.com/approvers/ichiyoAI/compare/ichiyo_ai-v1.0.1...ichiyo_ai-v1.1.0) (2023-08-12)


### Features

* chat_mode and reply_mode using gpt-4 ([2027f1d](https://github.com/approvers/ichiyoAI/commit/2027f1df67c86f67003764e07b1efb9e66f6ae7b))
* ease timeout ([ee8a751](https://github.com/approvers/ichiyoAI/commit/ee8a7512f1325f051bdea58d5ed9cae9ed2c9e01))
* support multi model ([2b9c21c](https://github.com/approvers/ichiyoAI/commit/2b9c21cee93db89e4a879fdfeb5db122bd9724a7))

## [1.0.1](https://github.com/approvers/ichiyoAI/compare/v1.0.0...v1.0.1) (2023-08-11)


### Bug Fixes

* dockerfile ([3d036d3](https://github.com/approvers/ichiyoAI/commit/3d036d3d65158b62ee5ae143e63f8763dd3f6d94))
* fix release ci ([f7ece6d](https://github.com/approvers/ichiyoAI/commit/f7ece6db5bd45fea8f6e6bf6f9a90cc522066ab4))

## [1.0.0](https://github.com/approvers/ichiyoAI/compare/v0.5.2...v1.0.0) (2023-08-11)


### ⚠ BREAKING CHANGES

- 環境変数の Key が変更されました.
  - `CHATGPT_API_TOKEN` → `OPENAI_API_KEY`
- 以下の機能は廃止しました.
  - `!direct` (指示モード)
  - `!hibiki` (響モード)

### Features

全コードを書き直し. 仕様を見直しました.

- 思考中メッセージ (`waiting_message`) は廃止され、代わりに入力中(Typing)を使用するようになりました。
- 返信モードを追加しました。限定的ですが、会話のコンテキストを維持できます。
- OpenAI API からのレスポンスが15秒以上かかった場合は、返信せずエラーになるようになりました。
