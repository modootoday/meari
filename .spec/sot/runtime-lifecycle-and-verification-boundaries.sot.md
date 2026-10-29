---
id: sot:meari/runtime-lifecycle-and-verification-boundaries
title: Runtime lifecycle and verification boundaries
status: active
created: 2026-10-29
updated: 2026-10-29
verify: manual
---
# Runtime lifecycle and verification boundaries

The portable core owns component instances, typed state, source schema, responsive revisions and build identity. Native and Web hosts own their renderer and event resources. A host must keep source replacement, DOM commit, rendered frame and coordinated Native/Web acknowledgement as separate stages.

## Application source replacement

`ApplicationDefinition` retains the original component and props with its session, generation, graph hash and target. `BuildBundle::application_definition` verifies output integrity and matches that stamp against the manifest. `FsArtifactStore::read_application` reads the immutable definition without acquiring the builder's writer lock.

Create a Native document with `InteractiveDocument::from_application` or a Web document with `WebDocument::mount_application`. Prepare a successor with `prepare_application_replacement`. It must use the same session/target and a newer, previously uncommitted generation. Retained replacement requires compatible source hooks/props and node identities. Title/initial viewport changes, structural remounts and CSS/asset replacement need further adapters.

`commit_reversible_application_replacement` returns an opaque committed token. Keep it until the coordinator has confirmed both hosts. `rollback_application_replacement` restores the previous source behavior while preserving compatible live user state and DOM. Tokens are bound to the document instance and source definition revision. Restoration advances the internal definition revision and retains the highest committed generation, rejecting old preparations and replayed failed generations. The convenience `commit_application_replacement` discards the rollback token.

Native regression coverage includes primitive and stateless documents, the seven existing composite families, dirty input/focus/accessibility values, failed/stale/foreign replacements and source restoration. New Web application replacement/rollback scenarios have WASM type-check coverage; browser execution is pending. This is not complete shadcn provider or React hook support.

## Rendered evidence

Native `render_application_frame` performs actual offscreen rasterization and returns build identity, source definition revision/schema, typed state, dimensions, byte count and pixel hash. A source change made outside its application commit invalidates this receipt path until the app is resynchronized. This SDK is available to a host or a CLI; it does not confirm OS window presentation or Browser paint.

The shared HMR coordinator still requires actual frame acknowledgements from both hosts. A successful local DOM commit or offscreen receipt alone cannot establish synchronized success. Source editor transport, paired host delivery, reconnect and cross-host rollback remain integration work.

## Host observations and media state

`ResponsiveSession::apply` owns renderer updates and recovery. `observe` accepts externally applied host geometry and atomically advances its revision and media baseline after all evaluations succeed, without resizing the external host. `StateProgram::bind_media` and `apply_media` stage boolean state updates with instance and revision checks.

WebDocument application mounts attach the existing host observer; other mounts can call `observe_host`. The application scheduler drains observations with `poll_host_environment` or explicitly refreshes with `refresh_host_environment`. `subscribe_media_state` connects a query to a boolean source-state slot. Web stages the responsive session, state, event projection and bindings, then publishes after DOM patch success. Unsubscription removes the binding; close/drop detach listeners and release the subscriptions.

Portable media-state tests execute locally. New Web DOM scenarios are type-checked but await browser execution. Native media-state integration has execution coverage for wide/narrow viewport changes, retained input/focus/accessibility and failed DOM patch recovery. Source `useMediaQuery`/`useIsMobile`, automatic frame scheduling, all preference-change listeners and full Native/Web parity remain unfinished.

## Source module documents

`ModuleProgram` owns source component instances separately from its renderer projection. Native `from_module_program` connects that projection to the same persistent component patch helper used by single-source state. DOM success precedes state publication. Engine/AccessKit/CLI input routes retain the owning module instance. Regression execution covers child state, props, dirty input/focus/accessibility, actual CPU raster and failed DOM patch non-publication.

Web `mount_modules` owns module state and click listeners; `module_snapshot` returns all instance states and `update_module_props` patches before publishing. These new Web scenarios have WASM type-check coverage; actual browser execution remains pending. Typed Web input, ReactNode/callback slots, keyed lists, graph source replacement/HMR and generated graph application bundles remain incomplete. See [consumer integration](consumer-integration.md).
