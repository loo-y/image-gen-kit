import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";

type ProviderProfile = {
  id: string;
  name: string;
  providerType: string;
  baseUrl: string;
  modelDefault: string;
  networkTimeoutMinutes: number;
  apiKeySaved: boolean;
  createdAt: number;
  updatedAt: number;
};

type Generation = {
  id: string;
  prompt: string;
  providerId: string;
  providerType: string;
  providerName: string;
  model: string;
  status: "running" | "succeeded" | "failed";
  size: string;
  quality: string;
  outputFormat: string;
  paramsJson: string;
  responseJson?: string | null;
  errorMessage?: string | null;
  revisedPrompt?: string | null;
  createdAt: number;
  completedAt?: number | null;
};

type GenerationOutput = {
  id: number;
  generationId: string;
  path: string;
  format: string;
  width?: number | null;
  height?: number | null;
  fileSize: number;
  outputIndex: number;
  createdAt: number;
};

type GenerationInputImage = {
  id: number;
  generationId: string;
  path: string;
  name: string;
  mimeType: string;
  fileSize: number;
  inputIndex: number;
  createdAt: number;
};

type GenerationDetail = {
  generation: Generation;
  outputs: GenerationOutput[];
  inputImages: GenerationInputImage[];
};

type AppBootstrap = {
  profiles: ProviderProfile[];
  generations: GenerationDetail[];
};

type StartedGeneration = {
  generationId: string;
  generation: GenerationDetail;
};

type EditInputImage = {
  id: string;
  name: string;
  mimeType: string;
  dataUrl: string;
  size: number;
};

type EditInputImagePayload = Omit<EditInputImage, "id">;

const sizeOptions = [
  { value: "1024x1024", label: "1024x1024 (square)" },
  { value: "1536x1024", label: "1536x1024 (landscape)" },
  { value: "1024x1536", label: "1024x1536 (portrait)" },
  { value: "2048x2048", label: "2048x2048 (2K square)" },
  { value: "2048x1152", label: "2048x1152 (2K landscape)" },
  { value: "3840x2160", label: "3840x2160 (4K landscape)" },
  { value: "2160x3840", label: "2160x3840 (4K portrait)" },
  { value: "auto", label: "auto (default)" },
];
const sizes = [...sizeOptions.map((option) => option.value), "custom"];
const qualities = ["auto", "low", "medium", "high"];
const formats = ["png", "jpeg", "webp"];
const moderationModes = ["auto", "low"];
const providerTypeOptions = [
  { value: "openai", label: "OpenAI compatible", disabled: false },
  { value: "google-nano-banana", label: "Google Nano Banana (TODO)", disabled: true },
];
const generationPollIntervalMs = 2500;
const generationPollAttempts = 400;
const maxEditImages = 16;
const maxEditImageBytes = 50 * 1024 * 1024;
const supportedEditMimeTypes = ["image/png", "image/jpeg", "image/webp"];

export default function App() {
  const [profiles, setProfiles] = useState<ProviderProfile[]>([]);
  const [activeProfileId, setActiveProfileId] = useState("openai-default");
  const [activeView, setActiveView] = useState<"generate" | "history" | "settings">("generate");
  const [providerAlias, setProviderAlias] = useState("OpenAI");
  const [providerType, setProviderType] = useState("openai");
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com/v1");
  const [model, setModel] = useState("gpt-image-2");
  const [apiKey, setApiKey] = useState("");
  const [saveApiKey, setSaveApiKey] = useState(false);
  const [networkTimeoutMinutes, setNetworkTimeoutMinutes] = useState(15);
  const [prompt, setPrompt] = useState("");
  const [size, setSize] = useState("auto");
  const [customWidth, setCustomWidth] = useState(1024);
  const [customHeight, setCustomHeight] = useState(1024);
  const [quality, setQuality] = useState("auto");
  const [outputFormat, setOutputFormat] = useState("png");
  const [outputCompression, setOutputCompression] = useState(90);
  const [moderation, setModeration] = useState("auto");
  const [generationMode, setGenerationMode] = useState<"generate" | "edit">("generate");
  const [editImages, setEditImages] = useState<EditInputImage[]>([]);
  const [isImageDropActive, setIsImageDropActive] = useState(false);
  const [debugMode, setDebugMode] = useState(true);
  const [history, setHistory] = useState<GenerationDetail[]>([]);
  const [historyQuery, setHistoryQuery] = useState("");
  const [thumbnailUrls, setThumbnailUrls] = useState<Record<string, string>>({});
  const [selected, setSelected] = useState<GenerationDetail | null>(null);
  const [imageDataUrl, setImageDataUrl] = useState("");
  const [previewImage, setPreviewImage] = useState<{ detail: GenerationDetail; dataUrl: string } | null>(null);
  const [detailGeneration, setDetailGeneration] = useState<GenerationDetail | null>(null);
  const [deleteCandidateId, setDeleteCandidateId] = useState<string | null>(null);
  const [isGenerating, setIsGenerating] = useState(false);
  const [isSavingProfile, setIsSavingProfile] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  const activeProfile = useMemo(
    () => profiles.find((profile) => profile.id === activeProfileId),
    [activeProfileId, profiles],
  );
  const deleteCandidate = useMemo(
    () => history.find((detail) => detail.generation.id === deleteCandidateId) ?? null,
    [deleteCandidateId, history],
  );

  const selectedSize = size === "custom" ? `${customWidth}x${customHeight}` : size;
  const compressionEnabled = outputFormat === "jpeg" || outputFormat === "webp";
  const selectedIdRef = useRef<string | null>(null);
  const activeViewRef = useRef(activeView);
  const editImagesRef = useRef(editImages);

  useEffect(() => {
    void bootstrap();
  }, []);

  useEffect(() => {
    selectedIdRef.current = selected?.generation.id ?? null;
  }, [selected]);

  useEffect(() => {
    activeViewRef.current = activeView;
  }, [activeView]);

  useEffect(() => {
    editImagesRef.current = editImages;
  }, [editImages]);

  useEffect(() => {
    if (!activeProfile) return;
    setProviderAlias(activeProfile.name);
    setProviderType(activeProfile.providerType);
    setBaseUrl(activeProfile.baseUrl);
    setModel(activeProfile.modelDefault);
    setNetworkTimeoutMinutes(activeProfile.networkTimeoutMinutes || 15);
    setSaveApiKey(activeProfile.apiKeySaved);
  }, [activeProfile]);

  useEffect(() => {
    if (activeView !== "history") return;
    let cancelled = false;
    const missing = history
      .filter((detail) => detail.outputs[0] && !thumbnailUrls[detail.generation.id])
      .slice(0, 80);

    if (missing.length === 0) return;

    void Promise.all(
      missing.map(async (detail) => {
        try {
          const dataUrl = await invoke<string>("read_image_data_url", {
            path: detail.outputs[0].path,
          });
          return [detail.generation.id, dataUrl] as const;
        } catch {
          return null;
        }
      }),
    ).then((entries) => {
      if (cancelled) return;
      setThumbnailUrls((current) => {
        const next = { ...current };
        for (const entry of entries) {
          if (entry) next[entry[0]] = entry[1];
        }
        return next;
      });
    });

    return () => {
      cancelled = true;
    };
  }, [activeView, history, thumbnailUrls]);

  useEffect(() => {
    const hasRunning = history.some((detail) => detail.generation.status === "running");
    if (!hasRunning) return;

    const timer = window.setInterval(() => {
      void refreshHistory();
    }, 2500);

    return () => window.clearInterval(timer);
  }, [history, historyQuery]);

  useEffect(() => {
    const onPaste = (event: ClipboardEvent) => {
      if (activeViewRef.current !== "generate") return;
      const files = imageFilesFromDataTransfer(event.clipboardData);
      if (files.length === 0) return;
      event.preventDefault();
      void appendEditImageFiles(files, "clipboard");
    };

    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (activeViewRef.current !== "generate") return;
        if (event.payload.type === "enter" || event.payload.type === "over") {
          setIsImageDropActive(true);
          return;
        }
        if (event.payload.type === "leave") {
          setIsImageDropActive(false);
          return;
        }
        setIsImageDropActive(false);
        if (event.payload.paths.length > 0) {
          void addDroppedImagePaths(event.payload.paths);
        }
      })
      .then((handler) => {
        if (cancelled) {
          handler();
          return;
        }
        unlisten = handler;
      })
      .catch(() => undefined);

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  async function bootstrap() {
    try {
      const boot = await invoke<AppBootstrap>("init_app");
      setProfiles(boot.profiles);
      setHistory(sortGenerations(boot.generations));
      if (boot.profiles[0]) {
        setActiveProfileId(boot.profiles[0].id);
      }
      if (boot.generations[0]) {
        await selectGeneration(boot.generations[0]);
      }
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function refreshHistory(query = historyQuery) {
    const generations = await invoke<GenerationDetail[]>("list_generations", {
      request: { query, limit: 80, offset: 0 },
    });
    const sorted = sortGenerations(generations);
    setHistory(sorted);
    return sorted;
  }

  async function saveProfile() {
    setIsSavingProfile(true);
    setError("");
    setNotice("");
    try {
      const alias = providerAlias.trim() || "OpenAI";
      const profile = await invoke<ProviderProfile>("save_provider_profile", {
        request: {
          id: activeProfileId || null,
          name: alias,
          providerType,
          baseUrl,
          modelDefault: model,
          networkTimeoutMinutes,
          apiKey,
          saveApiKey,
        },
      });
      setProfiles((current) => {
        const next = current.filter((item) => item.id !== profile.id);
        return [...next, profile].sort((a, b) => a.createdAt - b.createdAt);
      });
      setActiveProfileId(profile.id);
      setApiKey("");
      setNotice("Settings saved");
      return profile;
    } catch (err) {
      setError(errorMessage(err));
      throw err;
    } finally {
      setIsSavingProfile(false);
    }
  }

  async function generateImage() {
    if (!prompt.trim()) {
      setError("Prompt is required");
      return;
    }
    if (generationMode === "edit" && editImages.length === 0) {
      setError("Add at least one input image for image edit");
      return;
    }
    setIsGenerating(true);
    setError("");
    setNotice("");
    try {
      let profileId = activeProfileId;
      if (
        saveApiKey ||
        !activeProfile ||
        providerAlias.trim() !== activeProfile.name ||
        providerType !== activeProfile.providerType ||
        baseUrl !== activeProfile?.baseUrl ||
        model !== activeProfile?.modelDefault ||
        networkTimeoutMinutes !== (activeProfile?.networkTimeoutMinutes || 15)
      ) {
        const profile = await saveProfile();
        profileId = profile.id;
      }
      const started = await invoke<StartedGeneration>("start_generation", {
        request: {
          providerId: profileId,
          apiKeyOverride: apiKey,
          baseUrl,
          model,
          prompt,
          size: selectedSize,
          quality,
          outputFormat,
          outputCompression: compressionEnabled ? outputCompression : null,
          moderation,
          debugMode,
          networkTimeoutMinutes,
          inputImages: generationMode === "edit"
            ? editImages.map((image) => ({
                name: image.name,
                mimeType: image.mimeType,
                dataUrl: image.dataUrl,
              }))
            : [],
        },
      });
      setHistory((current) => upsertGeneration(current, started.generation));
      setSelected(started.generation);
      setImageDataUrl("");
      setNotice(generationMode === "edit" ? "Image edit started" : "Generation started");
      void pollGeneration(started.generationId);
    } catch (err) {
      setError(errorMessage(err));
      await refreshHistory().catch(() => undefined);
    } finally {
      setIsGenerating(false);
    }
  }

  function createProviderProfile() {
    setActiveProfileId("");
    setProviderAlias(uniqueProviderAlias("OpenAI", profiles));
    setProviderType("openai");
    setBaseUrl("https://api.openai.com/v1");
    setModel("gpt-image-2");
    setNetworkTimeoutMinutes(15);
    setApiKey("");
    setSaveApiKey(false);
    setActiveView("settings");
    setNotice("Configure the new provider, then save settings");
    setError("");
  }

  async function addEditImages(event: React.ChangeEvent<HTMLInputElement>) {
    const files = Array.from(event.currentTarget.files ?? []);
    event.currentTarget.value = "";
    if (files.length === 0) return;

    await appendEditImageFiles(files, "file picker");
  }

  async function appendEditImageFiles(files: File[], source: string) {
    setError("");
    setNotice("");
    try {
      const images = await Promise.all(files.map(readEditImageFile));
      appendEditImages(images, source);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  function appendEditImages(images: EditInputImagePayload[], source: string) {
    if (images.length === 0) return;
    const current = editImagesRef.current;
    if (current.length + images.length > maxEditImages) {
      setError(`Image edit supports up to ${maxEditImages} input images`);
      return;
    }
    const next = [
      ...current,
      ...images.map((image) => ({
        ...image,
        id: `${image.name}-${image.size}-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      })),
    ];
    editImagesRef.current = next;
    setEditImages(next);
    setGenerationMode("edit");
    setActiveView("generate");
    setNotice(`Added ${images.length} image${images.length === 1 ? "" : "s"} from ${source}`);
  }

  async function addDroppedImagePaths(paths: string[]) {
    setError("");
    setNotice("");
    try {
      const images = await invoke<EditInputImagePayload[]>("read_input_image_data_urls", {
        paths,
      });
      appendEditImages(images, "drop");
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function pasteEditImages() {
    setError("");
    setNotice("");
    try {
      const files = await readClipboardImageFiles();
      if (files.length === 0) {
        setNotice("Clipboard does not contain a supported image");
        return;
      }
      await appendEditImageFiles(files, "clipboard");
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  function handleUploadDragOver(event: React.DragEvent<HTMLElement>) {
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
    setIsImageDropActive(true);
  }

  function handleUploadDragLeave(event: React.DragEvent<HTMLElement>) {
    if (event.currentTarget.contains(event.relatedTarget as Node | null)) return;
    setIsImageDropActive(false);
  }

  function handleUploadDrop(event: React.DragEvent<HTMLElement>) {
    event.preventDefault();
    setIsImageDropActive(false);
    const files = imageFilesFromDataTransfer(event.dataTransfer);
    if (files.length > 0) {
      void appendEditImageFiles(files, "drop");
    }
  }

  function removeEditImage(id: string) {
    setEditImages((current) => {
      const next = current.filter((image) => image.id !== id);
      editImagesRef.current = next;
      return next;
    });
  }

  function clearEditImages() {
    editImagesRef.current = [];
    setEditImages([]);
  }

  async function pollGeneration(id: string) {
    for (let attempt = 0; attempt < generationPollAttempts; attempt += 1) {
      await wait(generationPollIntervalMs);
      try {
        const detail = await invoke<GenerationDetail | null>("get_generation", { id });
        if (!detail) continue;
        setHistory((current) => upsertGeneration(current, detail));
        setSelected((current) => (current?.generation.id === id ? detail : current));
        if (detail.generation.status !== "running") {
          if (detail.generation.status === "succeeded") {
            if (selectedIdRef.current === id) {
              setSelected(detail);
              await loadPreview(detail);
            }
            setNotice("Image generated");
          } else if (detail.generation.errorMessage) {
            setError(detail.generation.errorMessage);
          }
          return;
        }
      } catch {
        return;
      }
    }
  }

  async function selectGeneration(detail: GenerationDetail) {
    setSelected(detail);
    setPrompt(detail.generation.prompt);
    setModel(detail.generation.model);
    setSize(knownSize(detail.generation.size) ? detail.generation.size : "custom");
    if (!knownSize(detail.generation.size) && detail.generation.size.includes("x")) {
      const [width, height] = detail.generation.size.split("x").map(Number);
      if (Number.isFinite(width)) setCustomWidth(width);
      if (Number.isFinite(height)) setCustomHeight(height);
    }
    setQuality(detail.generation.quality);
    setOutputFormat(detail.generation.outputFormat);
    await loadPreview(detail);
  }

  async function loadPreview(detail: GenerationDetail) {
    const first = detail.outputs[0];
    if (!first) {
      setImageDataUrl("");
      return;
    }
    const dataUrl = await invoke<string>("read_image_data_url", { path: first.path });
    setImageDataUrl(dataUrl);
  }

  async function deleteSelected() {
    if (!selected) return;
    deleteGeneration(selected.generation.id);
  }

  async function revealSelected() {
    const path = selected?.outputs[0]?.path;
    if (!path) return;
    await invoke("reveal_image", { path });
  }

  async function revealDebugDirectory() {
    await invoke("reveal_debug_dir");
  }

  function deleteGeneration(id: string) {
    setDeleteCandidateId(id);
  }

  async function confirmDeleteGeneration() {
    if (!deleteCandidateId) return;
    const id = deleteCandidateId;
    await invoke("delete_generation", { id });
    setThumbnailUrls((current) => {
      const next = { ...current };
      delete next[id];
      return next;
    });
    if (selected?.generation.id === id) {
      setSelected(null);
      setImageDataUrl("");
    }
    if (detailGeneration?.generation.id === id) setDetailGeneration(null);
    if (previewImage?.detail.generation.id === id) setPreviewImage(null);
    setDeleteCandidateId(null);
    await refreshHistory();
  }

  async function revealGeneration(detail: GenerationDetail) {
    const path = detail.outputs[0]?.path;
    if (!path) return;
    await invoke("reveal_image", { path });
  }

  async function openGeneration(detail: GenerationDetail) {
    const path = detail.outputs[0]?.path;
    if (!path) return;
    await invoke("open_image", { path });
  }

  async function openImagesDirectory() {
    await invoke("open_images_dir");
  }

  async function minimizeToTray() {
    try {
      await invoke("minimize_to_tray");
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function quitApp() {
    try {
      await invoke("quit_app");
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function previewGeneration(detail: GenerationDetail) {
    await selectGeneration(detail);
    const first = detail.outputs[0];
    if (!first) return;
    const dataUrl = await invoke<string>("read_image_data_url", { path: first.path });
    setPreviewImage({ detail, dataUrl });
  }

  async function useGeneration(detail: GenerationDetail) {
    await selectGeneration(detail);
    if (detail.inputImages.length > 0) {
      clearEditImages();
      try {
        const restoredImages = await loadHistoryInputImages(detail);
        editImagesRef.current = restoredImages;
        setEditImages(restoredImages);
        setGenerationMode("edit");
        setNotice(`Restored ${restoredImages.length} input image${restoredImages.length === 1 ? "" : "s"} from history`);
      } catch (err) {
        setError(errorMessage(err));
      }
    } else {
      clearEditImages();
      setGenerationMode("generate");
    }
    setActiveView("generate");
  }

  async function onHistorySearch(value: string) {
    setHistoryQuery(value);
    try {
      await refreshHistory(value);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  return (
    <main className="shell">
      <aside className="rail">
        <div className="brandMark">IG</div>
        <button
          className={activeView === "generate" ? "railButton active" : "railButton"}
          onClick={() => setActiveView("generate")}
        >
          Generate
        </button>
        <button
          className={activeView === "history" ? "railButton active" : "railButton"}
          onClick={() => setActiveView("history")}
        >
          History
        </button>
        <button
          className={activeView === "settings" ? "railButton active" : "railButton"}
          onClick={() => setActiveView("settings")}
        >
          Settings
        </button>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <h1>Image Gen Kit</h1>
            <p>{activeProfile?.name ?? providerAlias} · {model}</p>
          </div>
          <div className="topbarActions">
            <label className="profileSelect">
              <span>Provider</span>
              <select value={activeProfileId} onChange={(event) => setActiveProfileId(event.target.value)}>
                {!activeProfileId && <option value="">Unsaved provider</option>}
                {profiles.map((profile) => (
                  <option key={profile.id} value={profile.id}>
                    {profile.name}
                  </option>
                ))}
              </select>
            </label>
            <button className="newProviderButton" onClick={createProviderProfile}>
              New provider
            </button>
            <button className="saveSettingsButton" onClick={saveProfile} disabled={isSavingProfile}>
              {isSavingProfile ? "Saving" : "Save settings"}
            </button>
            <button className="trayButton" onClick={minimizeToTray}>
              Tray
            </button>
            <button className="quitButton" onClick={quitApp}>
              Quit
            </button>
          </div>
        </header>

        {(error || notice) && (
          <div className={error ? "message error" : "message"}>
            {error || notice}
          </div>
        )}

        {activeView === "settings" ? (
          <SettingsView
            providerAlias={providerAlias}
            setProviderAlias={setProviderAlias}
            providerType={providerType}
            setProviderType={setProviderType}
            baseUrl={baseUrl}
            setBaseUrl={setBaseUrl}
            model={model}
            setModel={setModel}
            networkTimeoutMinutes={networkTimeoutMinutes}
            setNetworkTimeoutMinutes={setNetworkTimeoutMinutes}
            apiKey={apiKey}
            setApiKey={setApiKey}
            saveApiKey={saveApiKey}
            setSaveApiKey={setSaveApiKey}
            activeProfile={activeProfile}
            onCreateProvider={createProviderProfile}
          />
        ) : activeView === "history" ? (
          <GalleryHistoryView
            history={history}
            query={historyQuery}
            thumbnailUrls={thumbnailUrls}
            selectedId={selected?.generation.id}
            onQuery={onHistorySearch}
            onSelect={selectGeneration}
            onPreview={previewGeneration}
            onOpen={openGeneration}
            onUse={useGeneration}
            onDetail={setDetailGeneration}
            onReveal={revealGeneration}
            onOpenFolder={openImagesDirectory}
            onDelete={deleteGeneration}
          />
        ) : (
          <div className="contentGrid">
            <section className="composer">
              <div className="promptHeader">
                <span>Prompt</span>
                <span>{prompt.length} chars</span>
              </div>
              <textarea
                value={prompt}
                onChange={(event) => setPrompt(event.target.value)}
                placeholder="A quiet product photo of a modular image-generation desk setup, natural light, precise materials"
              />

              <div className="modeSwitch">
                <button
                  type="button"
                  className={generationMode === "generate" ? "modeButton active" : "modeButton"}
                  onClick={() => setGenerationMode("generate")}
                >
                  Create
                </button>
                <button
                  type="button"
                  className={generationMode === "edit" ? "modeButton active" : "modeButton"}
                  onClick={() => setGenerationMode("edit")}
                >
                  Edit from image
                </button>
              </div>

              {generationMode === "edit" && (
                <section className="editPanel">
                  <div className="uploadTools">
                    <label
                      className={isImageDropActive ? "uploadBox dropActive" : "uploadBox"}
                      onDragEnter={handleUploadDragOver}
                      onDragOver={handleUploadDragOver}
                      onDragLeave={handleUploadDragLeave}
                      onDrop={handleUploadDrop}
                    >
                      <input
                        type="file"
                        multiple
                        accept="image/png,image/jpeg,image/webp"
                        onChange={addEditImages}
                      />
                      <span>{isImageDropActive ? "Drop images here" : "Choose or drop input images"}</span>
                      <small>PNG, JPEG, or WebP · paste with Ctrl+V · up to {maxEditImages} files</small>
                    </label>
                    <button type="button" className="pasteImageButton" onClick={pasteEditImages}>
                      Paste image
                    </button>
                  </div>

                  {editImages.length > 0 ? (
                    <div className="editImageGrid">
                      {editImages.map((image) => (
                        <article key={image.id} className="editImageCard">
                          <img src={image.dataUrl} alt={image.name} />
                          <div>
                            <strong>{image.name}</strong>
                            <small>{formatBytes(image.size)}</small>
                          </div>
                          <button type="button" onClick={() => removeEditImage(image.id)}>
                            Remove
                          </button>
                        </article>
                      ))}
                      <button type="button" className="clearImagesButton" onClick={clearEditImages}>
                        Clear all
                      </button>
                    </div>
                  ) : (
                    <p className="editHint">Add at least one input image to call the Image Edits API.</p>
                  )}
                </section>
              )}

              <div className="controlGrid">
                <Field label="Model">
                  <input value={model} onChange={(event) => setModel(event.target.value)} />
                </Field>
                <Field label="Size">
                  <select value={size} onChange={(event) => setSize(event.target.value)}>
                    {sizeOptions.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                    <option value="custom">custom</option>
                  </select>
                </Field>
                {size === "custom" && (
                  <>
                    <Field label="Width">
                      <input
                        type="number"
                        min={256}
                        max={3840}
                        step={16}
                        value={customWidth}
                        onChange={(event) => setCustomWidth(Number(event.target.value))}
                      />
                    </Field>
                    <Field label="Height">
                      <input
                        type="number"
                        min={256}
                        max={3840}
                        step={16}
                        value={customHeight}
                        onChange={(event) => setCustomHeight(Number(event.target.value))}
                      />
                    </Field>
                  </>
                )}
                <Field label="Quality">
                  <select value={quality} onChange={(event) => setQuality(event.target.value)}>
                    {qualities.map((item) => (
                      <option key={item} value={item}>
                        {item}
                      </option>
                    ))}
                  </select>
                </Field>
                <Field label="Format">
                  <select value={outputFormat} onChange={(event) => setOutputFormat(event.target.value)}>
                    {formats.map((item) => (
                      <option key={item} value={item}>
                        {item}
                      </option>
                    ))}
                  </select>
                </Field>
                <Field label="Compression">
                  <input
                    type="range"
                    min={0}
                    max={100}
                    value={outputCompression}
                    disabled={!compressionEnabled}
                    onChange={(event) => setOutputCompression(Number(event.target.value))}
                  />
                  <span className="rangeValue">{compressionEnabled ? outputCompression : "png"}</span>
                </Field>
                <Field label="Moderation">
                  <select value={moderation} onChange={(event) => setModeration(event.target.value)}>
                    {moderationModes.map((item) => (
                      <option key={item} value={item}>
                        {item}
                      </option>
                    ))}
                  </select>
                </Field>
              </div>

              <div className="debugRow">
                <label className="checkRow">
                  <input
                    type="checkbox"
                    checked={debugMode}
                    onChange={(event) => setDebugMode(event.target.checked)}
                  />
                  <span>Debug mode</span>
                </label>
                <button className="secondaryButton" onClick={revealDebugDirectory}>
                  Debug files
                </button>
              </div>

              <div className="runRow">
                <button className="primaryButton" onClick={generateImage} disabled={isGenerating}>
                  {isGenerating ? "Working" : generationMode === "edit" ? "Edit image" : "Generate image"}
                </button>
                <span>{generationMode} · {selectedSize} · {quality} · {outputFormat}</span>
              </div>
            </section>

            <HistoryView
              history={history}
              query={historyQuery}
              onQuery={onHistorySearch}
              selectedId={selected?.generation.id}
              onSelect={selectGeneration}
            />

            <Inspector
              detail={selected}
              imageDataUrl={imageDataUrl}
              onOpen={() => selected && openGeneration(selected)}
              onReveal={revealSelected}
              onDelete={deleteSelected}
            />
          </div>
        )}
        {previewImage && (
          <ImagePreviewModal
            detail={previewImage.detail}
            dataUrl={previewImage.dataUrl}
            onClose={() => setPreviewImage(null)}
            onOpen={() => openGeneration(previewImage.detail)}
            onReveal={() => revealGeneration(previewImage.detail)}
          />
        )}
        {detailGeneration && (
          <GenerationDetailModal
            detail={detailGeneration}
            onClose={() => setDetailGeneration(null)}
            onOpen={() => openGeneration(detailGeneration)}
            onReveal={() => revealGeneration(detailGeneration)}
          />
        )}
        {deleteCandidateId && (
          <DeleteConfirmModal
            detail={deleteCandidate}
            fallbackId={deleteCandidateId}
            onCancel={() => setDeleteCandidateId(null)}
            onConfirm={confirmDeleteGeneration}
          />
        )}
      </section>
    </main>
  );
}

function DeleteConfirmModal(props: {
  detail: GenerationDetail | null;
  fallbackId: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const prompt = props.detail?.generation.prompt.trim() || props.fallbackId;
  return (
    <div className="modalOverlay" onClick={props.onCancel}>
      <section className="confirmModal" onClick={(event) => event.stopPropagation()}>
        <header>
          <h2>Delete generation?</h2>
          <p>This will remove the history record and saved image files.</p>
        </header>
        <p className="confirmPrompt">{prompt}</p>
        <div className="confirmActions">
          <button className="secondaryButton" onClick={props.onCancel}>
            Cancel
          </button>
          <button className="dangerButton" onClick={props.onConfirm}>
            Delete
          </button>
        </div>
      </section>
    </div>
  );
}

function GalleryHistoryView(props: {
  history: GenerationDetail[];
  query: string;
  thumbnailUrls: Record<string, string>;
  selectedId?: string;
  onQuery: (value: string) => void;
  onSelect: (detail: GenerationDetail) => void;
  onPreview: (detail: GenerationDetail) => void;
  onOpen: (detail: GenerationDetail) => void;
  onUse: (detail: GenerationDetail) => void;
  onDetail: (detail: GenerationDetail) => void;
  onReveal: (detail: GenerationDetail) => void;
  onOpenFolder: () => void;
  onDelete: (id: string) => void;
}) {
  return (
    <section className="historyGallery">
      <div className="galleryToolbar">
        <div>
          <h2>History</h2>
          <p>{props.history.length} generations</p>
        </div>
        <input
          value={props.query}
          placeholder="Search prompt, model, provider"
          onChange={(event) => props.onQuery(event.target.value)}
        />
        <button className="secondaryButton" onClick={props.onOpenFolder}>
          Image folder
        </button>
      </div>

      <div className="galleryGrid">
        {props.history.map((detail) => {
          const output = detail.outputs[0];
          const thumbnail = props.thumbnailUrls[detail.generation.id];
          return (
            <article
              key={detail.generation.id}
              className={props.selectedId === detail.generation.id ? "galleryCard selected" : "galleryCard"}
            >
              <button className="galleryPreview" onClick={() => (output ? props.onPreview(detail) : props.onSelect(detail))}>
                {thumbnail ? (
                  <img src={thumbnail} alt="Generated output thumbnail" />
                ) : (
                  <span className={`galleryPlaceholder ${detail.generation.status}`}>
                    {displayStatus(detail.generation.status)}
                  </span>
                )}
              </button>
              <div className="galleryMeta">
                <p className="galleryPrompt">{detail.generation.prompt || "Untitled prompt"}</p>
                <div className="galleryStats">
                  <span>{detail.generation.model}</span>
                  <span>{detail.generation.size}</span>
                  {detail.inputImages.length > 0 && <span>{detail.inputImages.length} input image{detail.inputImages.length > 1 ? "s" : ""}</span>}
                  <span>{formatTime(detail.generation.createdAt)}</span>
                </div>
                <div className="galleryActions">
                  <button className="smallButton" onClick={() => props.onPreview(detail)} disabled={!output}>
                    Preview
                  </button>
                  <button className="smallButton" onClick={() => props.onOpen(detail)} disabled={!output}>
                    Open
                  </button>
                  <button className="smallButton" onClick={() => props.onDetail(detail)}>
                    Detail
                  </button>
                  <button className="smallButton" onClick={() => props.onUse(detail)}>
                    Use
                  </button>
                  <button className="smallButton" onClick={() => props.onReveal(detail)} disabled={!output}>
                    Reveal
                  </button>
                  <button className="smallDangerButton" onClick={() => props.onDelete(detail.generation.id)}>
                    Delete
                  </button>
                </div>
              </div>
            </article>
          );
        })}
        {props.history.length === 0 && <div className="galleryEmpty">No generations</div>}
      </div>
    </section>
  );
}

function SettingsView(props: {
  providerAlias: string;
  setProviderAlias: (value: string) => void;
  providerType: string;
  setProviderType: (value: string) => void;
  baseUrl: string;
  setBaseUrl: (value: string) => void;
  model: string;
  setModel: (value: string) => void;
  networkTimeoutMinutes: number;
  setNetworkTimeoutMinutes: (value: number) => void;
  apiKey: string;
  setApiKey: (value: string) => void;
  saveApiKey: boolean;
  setSaveApiKey: (value: boolean) => void;
  activeProfile?: ProviderProfile;
  onCreateProvider: () => void;
}) {
  return (
    <section className="settingsPane">
      <div className="settingsGroup">
        <div className="settingsGroupHeader">
          <div>
            <h2>Provider settings</h2>
            <p>{props.activeProfile ? `Editing ${props.activeProfile.name}` : "New unsaved provider"}</p>
          </div>
          <button type="button" className="secondaryButton" onClick={props.onCreateProvider}>
            New provider
          </button>
        </div>
        <Field label="Provider alias">
          <input value={props.providerAlias} onChange={(event) => props.setProviderAlias(event.target.value)} />
        </Field>
        <Field label="Provider type">
          <select value={props.providerType} onChange={(event) => props.setProviderType(event.target.value)}>
            {providerTypeOptions.map((option) => (
              <option key={option.value} value={option.value} disabled={option.disabled}>
                {option.label}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Base URL">
          <input value={props.baseUrl} onChange={(event) => props.setBaseUrl(event.target.value)} />
        </Field>
        <Field label="Default model">
          <input value={props.model} onChange={(event) => props.setModel(event.target.value)} />
        </Field>
        <Field label="Network timeout (minutes)">
          <input
            type="number"
            min={1}
            max={120}
            value={props.networkTimeoutMinutes}
            onChange={(event) => props.setNetworkTimeoutMinutes(clampTimeoutMinutes(Number(event.target.value)))}
          />
        </Field>
        <Field label="API key">
          <input
            type="password"
            value={props.apiKey}
            placeholder={props.activeProfile?.apiKeySaved ? "Saved key available" : "sk-..."}
            onChange={(event) => props.setApiKey(event.target.value)}
          />
        </Field>
        <label className="checkRow">
          <input
            type="checkbox"
            checked={props.saveApiKey}
            onChange={(event) => props.setSaveApiKey(event.target.checked)}
          />
          <span>Remember API key locally</span>
        </label>
      </div>
      <div className="providerNote">
        <h2>Provider model</h2>
        <p>Provider alias is the display name shown in the top-right switcher and history records.</p>
        <p>Only OpenAI-compatible image generation is implemented now. Google Nano Banana is intentionally left as a provider-type TODO so it can be added without changing history records.</p>
      </div>
    </section>
  );
}

function HistoryView(props: {
  history: GenerationDetail[];
  query: string;
  onQuery: (value: string) => void;
  selectedId?: string;
  onSelect: (detail: GenerationDetail) => void;
}) {
  return (
    <section className="historyPane">
      <div className="historyHeader">
        <span>History</span>
        <input
          value={props.query}
          placeholder="Search prompt or model"
          onChange={(event) => props.onQuery(event.target.value)}
        />
      </div>
      <div className="historyList">
        {props.history.map((detail) => (
          <button
            key={detail.generation.id}
            className={props.selectedId === detail.generation.id ? "historyItem selected" : "historyItem"}
            onClick={() => props.onSelect(detail)}
          >
            <span className={`statusDot ${detail.generation.status}`} />
            <span className="historyText">
              <strong>{detail.generation.prompt || "Untitled prompt"}</strong>
              <small>{detail.generation.model} · {displayStatus(detail.generation.status)} · {formatTime(detail.generation.createdAt)}</small>
            </span>
          </button>
        ))}
        {props.history.length === 0 && <div className="emptyState">No generations</div>}
      </div>
    </section>
  );
}

function ImagePreviewModal(props: {
  detail: GenerationDetail;
  dataUrl: string;
  onClose: () => void;
  onOpen: () => void;
  onReveal: () => void;
}) {
  return (
    <div className="modalOverlay" onClick={props.onClose}>
      <section className="imageModal" onClick={(event) => event.stopPropagation()}>
        <header className="modalHeader">
          <div>
            <h2>Preview</h2>
            <p>{props.detail.generation.model} · {props.detail.generation.size}</p>
          </div>
          <div className="modalActions">
            <button className="secondaryButton" onClick={props.onOpen}>
              Open
            </button>
            <button className="secondaryButton" onClick={props.onReveal}>
              Reveal
            </button>
            <button className="secondaryButton" onClick={props.onClose}>
              Close
            </button>
          </div>
        </header>
        <div className="modalImageWrap">
          <img src={props.dataUrl} alt="Generated output preview" />
        </div>
        <p className="modalPrompt">{props.detail.generation.prompt}</p>
      </section>
    </div>
  );
}

function InputImagesSection(props: { images: GenerationInputImage[] }) {
  const [dataUrls, setDataUrls] = useState<Record<number, string>>({});
  const imageKey = props.images.map((image) => `${image.id}:${image.path}`).join("|");

  useEffect(() => {
    if (props.images.length === 0) {
      setDataUrls({});
      return;
    }

    let cancelled = false;
    setDataUrls({});
    void Promise.all(
      props.images.map(async (image) => {
        try {
          const dataUrl = await invoke<string>("read_image_data_url", { path: image.path });
          return [image.id, dataUrl] as const;
        } catch {
          return [image.id, ""] as const;
        }
      }),
    ).then((entries) => {
      if (cancelled) return;
      setDataUrls(Object.fromEntries(entries));
    });

    return () => {
      cancelled = true;
    };
  }, [imageKey]);

  if (props.images.length === 0) {
    return (
      <section className="detailSection inputImagesSection">
        <h3>Input images</h3>
        <p className="emptyDetailText">No input images</p>
      </section>
    );
  }

  return (
    <section className="detailSection inputImagesSection">
      <h3>Input images</h3>
      <div className="inputImageGrid">
        {props.images.map((image) => (
          <figure key={image.id} className="inputImageCard">
            <div className="inputImagePreview">
              {dataUrls[image.id] ? (
                <img src={dataUrls[image.id]} alt={`Input ${image.inputIndex + 1}`} />
              ) : (
                <span>Unable to preview</span>
              )}
            </div>
            <figcaption>
              <strong>{image.name || `Input ${image.inputIndex + 1}`}</strong>
              <small>{image.mimeType} · {formatBytes(image.fileSize)}</small>
            </figcaption>
          </figure>
        ))}
      </div>
    </section>
  );
}

function GenerationDetailModal(props: {
  detail: GenerationDetail;
  onClose: () => void;
  onOpen: () => void;
  onReveal: () => void;
}) {
  const output = props.detail.outputs[0];
  return (
    <div className="modalOverlay" onClick={props.onClose}>
      <section className="detailModal" onClick={(event) => event.stopPropagation()}>
        <header className="modalHeader">
          <div>
            <h2>Generation detail</h2>
            <p>{props.detail.generation.model} · {displayStatus(props.detail.generation.status)} · {formatTime(props.detail.generation.createdAt)}</p>
          </div>
          <div className="modalActions">
            <button className="secondaryButton" onClick={props.onOpen} disabled={!output}>
              Open
            </button>
            <button className="secondaryButton" onClick={props.onReveal} disabled={!output}>
              Reveal
            </button>
            <button className="secondaryButton" onClick={props.onClose}>
              Close
            </button>
          </div>
        </header>

        <div className="detailBody">
          <section className="detailSection">
            <h3>Prompt</h3>
            <p className="detailPrompt">{props.detail.generation.prompt}</p>
          </section>

          <div className="detailInfoRow">
            <InputImagesSection images={props.detail.inputImages} />

            <section className="detailSection metadataSection">
              <h3>Metadata</h3>
              <dl>
                <dt>ID</dt>
                <dd>{props.detail.generation.id}</dd>
                <dt>Provider</dt>
                <dd>{props.detail.generation.providerName}</dd>
                <dt>Size</dt>
                <dd>{props.detail.generation.size}</dd>
                <dt>Quality</dt>
                <dd>{props.detail.generation.quality}</dd>
                <dt>Format</dt>
                <dd>{props.detail.generation.outputFormat}</dd>
                <dt>Completed</dt>
                <dd>{props.detail.generation.completedAt ? formatTime(props.detail.generation.completedAt) : "-"}</dd>
              </dl>
            </section>
          </div>

          {props.detail.generation.errorMessage && (
            <section className="detailSection errorDetail">
              <h3>Error</h3>
              <pre>{props.detail.generation.errorMessage}</pre>
            </section>
          )}

          <div className="detailPayloadRow">
            <section className="detailSection requestSection">
              <h3>Request</h3>
              <pre>{prettyJson(props.detail.generation.paramsJson)}</pre>
            </section>

            <section className="detailSection responseSection">
              <h3>Response</h3>
              <pre>{props.detail.generation.responseJson ? prettyJson(props.detail.generation.responseJson) : "No response captured"}</pre>
            </section>
          </div>
        </div>
      </section>
    </div>
  );
}

function Inspector(props: {
  detail: GenerationDetail | null;
  imageDataUrl: string;
  onOpen: () => void;
  onReveal: () => void;
  onDelete: () => void;
}) {
  const output = props.detail?.outputs[0];
  return (
    <aside className="inspector">
      <div className="previewSurface">
        {props.imageDataUrl ? (
          <img src={props.imageDataUrl} alt="Generated output" />
        ) : (
          <div className="emptyPreview">No image selected</div>
        )}
      </div>

      <div className="metaBlock">
        <h2>Result</h2>
        <dl>
          <dt>Status</dt>
          <dd>{props.detail ? displayStatus(props.detail.generation.status) : "idle"}</dd>
          <dt>Model</dt>
          <dd>{props.detail?.generation.model ?? "-"}</dd>
          <dt>Size</dt>
          <dd>{props.detail?.generation.size ?? "-"}</dd>
          <dt>Format</dt>
          <dd>{props.detail?.generation.outputFormat ?? "-"}</dd>
          <dt>File</dt>
          <dd>{output ? compactPath(output.path) : "-"}</dd>
        </dl>
        {props.detail?.generation.errorMessage && (
          <p className="inlineError">{props.detail.generation.errorMessage}</p>
        )}
        {props.detail?.generation.revisedPrompt && (
          <p className="revisedPrompt">{props.detail.generation.revisedPrompt}</p>
        )}
      </div>

      <div className="inspectorActions">
        <button className="secondaryButton" onClick={props.onOpen} disabled={!output}>
          Open
        </button>
        <button className="secondaryButton" onClick={props.onReveal} disabled={!output}>
          Reveal
        </button>
        <button className="dangerButton" onClick={props.onDelete} disabled={!props.detail}>
          Delete
        </button>
      </div>
    </aside>
  );
}

function Field(props: { label: string; children: React.ReactNode }) {
  return (
    <label className="field">
      <span>{props.label}</span>
      {props.children}
    </label>
  );
}

function readEditImageFile(file: File): Promise<EditInputImagePayload> {
  const mimeType = supportedMimeTypeForFile(file);
  if (!mimeType) {
    return Promise.reject(new Error("Input images must be PNG, JPEG, or WebP"));
  }
  if (file.size > maxEditImageBytes) {
    return Promise.reject(new Error("Each input image must be 50MB or smaller"));
  }

  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result !== "string") {
        reject(new Error("Unable to read input image"));
        return;
      }
      resolve({
        name: file.name,
        mimeType,
        dataUrl: reader.result,
        size: file.size,
      });
    };
    reader.onerror = () => reject(new Error("Unable to read input image"));
    reader.readAsDataURL(file);
  });
}

async function readClipboardImageFiles() {
  if (!navigator.clipboard || !("read" in navigator.clipboard)) {
    throw new Error("Clipboard image reads are not available. Use Ctrl+V after copying an image.");
  }

  const clipboardItems = await navigator.clipboard.read();
  const files: File[] = [];
  for (const item of clipboardItems) {
    const type = item.types.find((candidate) => supportedEditMimeTypes.includes(normalizeEditMimeType(candidate)));
    if (!type) continue;
    const blob = await item.getType(type);
    const mimeType = normalizeEditMimeType(blob.type || type);
    files.push(
      new File([blob], `clipboard-${Date.now()}-${files.length}.${extensionForMimeType(mimeType)}`, {
        type: mimeType,
      }),
    );
  }
  return files;
}

function imageFilesFromDataTransfer(data: DataTransfer | null) {
  if (!data) return [];

  const itemFiles = Array.from(data.items ?? [])
    .filter((item) => item.kind === "file")
    .map((item) => item.getAsFile())
    .filter((file): file is File => Boolean(file))
    .filter((file) => Boolean(supportedMimeTypeForFile(file)));

  if (itemFiles.length > 0) return itemFiles;

  return Array.from(data.files ?? []).filter((file) => Boolean(supportedMimeTypeForFile(file)));
}

function supportedMimeTypeForFile(file: File) {
  const fromType = normalizeEditMimeType(file.type);
  if (supportedEditMimeTypes.includes(fromType)) return fromType;

  const extension = file.name.split(".").pop()?.toLowerCase();
  if (extension === "png") return "image/png";
  if (extension === "jpg" || extension === "jpeg") return "image/jpeg";
  if (extension === "webp") return "image/webp";
  return "";
}

function normalizeEditMimeType(value: string) {
  const normalized = value.trim().toLowerCase();
  if (normalized === "image/jpg" || normalized === "image/pjpeg") return "image/jpeg";
  return normalized;
}

function extensionForMimeType(mimeType: string) {
  if (mimeType === "image/jpeg") return "jpg";
  if (mimeType === "image/webp") return "webp";
  return "png";
}

function errorMessage(err: unknown) {
  if (err instanceof Error) return err.message;
  return String(err);
}

function knownSize(value: string) {
  return sizes.includes(value);
}

async function loadHistoryInputImages(detail: GenerationDetail): Promise<EditInputImage[]> {
  const images = [...detail.inputImages].sort((a, b) => a.inputIndex - b.inputIndex);
  return Promise.all(
    images.map(async (image) => {
      const dataUrl = await invoke<string>("read_image_data_url", { path: image.path });
      return {
        id: `history-${image.id}-${Date.now()}-${Math.random().toString(36).slice(2)}`,
        name: image.name || `Input ${image.inputIndex + 1}`,
        mimeType: image.mimeType,
        dataUrl,
        size: image.fileSize,
      };
    }),
  );
}

function upsertGeneration(current: GenerationDetail[], detail: GenerationDetail) {
  return sortGenerations([detail, ...current.filter((item) => item.generation.id !== detail.generation.id)]);
}

function sortGenerations(items: GenerationDetail[]) {
  return [...items].sort((a, b) => {
    const byCreatedAt = b.generation.createdAt - a.generation.createdAt;
    if (byCreatedAt !== 0) return byCreatedAt;
    return b.generation.id.localeCompare(a.generation.id);
  });
}

function uniqueProviderAlias(base: string, profiles: ProviderProfile[]) {
  const names = new Set(profiles.map((profile) => profile.name.toLowerCase()));
  if (!names.has(base.toLowerCase())) return base;
  for (let index = 2; index < 100; index += 1) {
    const candidate = `${base} ${index}`;
    if (!names.has(candidate.toLowerCase())) return candidate;
  }
  return `${base} ${Date.now()}`;
}

function formatTime(value: number) {
  return new Date(value).toLocaleString(undefined, {
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function compactPath(path: string) {
  const parts = path.split(/[\\/]/);
  return parts.slice(-3).join("/");
}

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

function prettyJson(value: string) {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}

function clampTimeoutMinutes(value: number) {
  if (!Number.isFinite(value)) return 15;
  return Math.min(120, Math.max(1, Math.round(value)));
}

function wait(ms: number) {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function displayStatus(status: Generation["status"]) {
  if (status === "running") return "generating";
  return status;
}
