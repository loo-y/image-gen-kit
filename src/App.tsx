import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type ProviderProfile = {
  id: string;
  name: string;
  providerType: string;
  baseUrl: string;
  modelDefault: string;
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

type GenerationDetail = {
  generation: Generation;
  outputs: GenerationOutput[];
};

type AppBootstrap = {
  profiles: ProviderProfile[];
  generations: GenerationDetail[];
};

const sizes = ["auto", "1024x1024", "1536x1024", "1024x1536", "custom"];
const qualities = ["auto", "low", "medium", "high"];
const formats = ["png", "jpeg", "webp"];
const moderationModes = ["auto", "low"];

export default function App() {
  const [profiles, setProfiles] = useState<ProviderProfile[]>([]);
  const [activeProfileId, setActiveProfileId] = useState("openai-default");
  const [activeView, setActiveView] = useState<"generate" | "history" | "settings">("generate");
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com/v1");
  const [model, setModel] = useState("gpt-image-2");
  const [apiKey, setApiKey] = useState("");
  const [saveApiKey, setSaveApiKey] = useState(false);
  const [prompt, setPrompt] = useState("");
  const [size, setSize] = useState("1024x1024");
  const [customWidth, setCustomWidth] = useState(1024);
  const [customHeight, setCustomHeight] = useState(1024);
  const [quality, setQuality] = useState("auto");
  const [outputFormat, setOutputFormat] = useState("png");
  const [outputCompression, setOutputCompression] = useState(90);
  const [moderation, setModeration] = useState("auto");
  const [debugMode, setDebugMode] = useState(false);
  const [history, setHistory] = useState<GenerationDetail[]>([]);
  const [historyQuery, setHistoryQuery] = useState("");
  const [thumbnailUrls, setThumbnailUrls] = useState<Record<string, string>>({});
  const [selected, setSelected] = useState<GenerationDetail | null>(null);
  const [imageDataUrl, setImageDataUrl] = useState("");
  const [isGenerating, setIsGenerating] = useState(false);
  const [isSavingProfile, setIsSavingProfile] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  const activeProfile = useMemo(
    () => profiles.find((profile) => profile.id === activeProfileId),
    [activeProfileId, profiles],
  );

  const selectedSize = size === "custom" ? `${customWidth}x${customHeight}` : size;
  const compressionEnabled = outputFormat === "jpeg" || outputFormat === "webp";

  useEffect(() => {
    void bootstrap();
  }, []);

  useEffect(() => {
    if (!activeProfile) return;
    setBaseUrl(activeProfile.baseUrl);
    setModel(activeProfile.modelDefault);
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

  async function bootstrap() {
    try {
      const boot = await invoke<AppBootstrap>("init_app");
      setProfiles(boot.profiles);
      setHistory(boot.generations);
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
    setHistory(generations);
    return generations;
  }

  async function saveProfile() {
    setIsSavingProfile(true);
    setError("");
    setNotice("");
    try {
      const profile = await invoke<ProviderProfile>("save_provider_profile", {
        request: {
          id: activeProfileId,
          name: activeProfile?.name ?? "OpenAI",
          providerType: activeProfile?.providerType ?? "openai",
          baseUrl,
          modelDefault: model,
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
    setIsGenerating(true);
    setError("");
    setNotice("");
    try {
      let profileId = activeProfileId;
      if (saveApiKey || baseUrl !== activeProfile?.baseUrl || model !== activeProfile?.modelDefault) {
        const profile = await saveProfile();
        profileId = profile.id;
      }
      const detail = await invoke<GenerationDetail>("generate_image", {
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
        },
      });
      setSelected(detail);
      await loadPreview(detail);
      await refreshHistory();
      setActiveView("generate");
      setNotice("Image generated");
    } catch (err) {
      setError(errorMessage(err));
      await refreshHistory().catch(() => undefined);
    } finally {
      setIsGenerating(false);
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
    await deleteGeneration(selected.generation.id);
    setSelected(null);
    setImageDataUrl("");
  }

  async function revealSelected() {
    const path = selected?.outputs[0]?.path;
    if (!path) return;
    await invoke("reveal_image", { path });
  }

  async function revealDebugDirectory() {
    await invoke("reveal_debug_dir");
  }

  async function deleteGeneration(id: string) {
    await invoke("delete_generation", { id });
    setThumbnailUrls((current) => {
      const next = { ...current };
      delete next[id];
      return next;
    });
    await refreshHistory();
  }

  async function revealGeneration(detail: GenerationDetail) {
    const path = detail.outputs[0]?.path;
    if (!path) return;
    await invoke("reveal_image", { path });
  }

  async function useGeneration(detail: GenerationDetail) {
    await selectGeneration(detail);
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
            <p>{activeProfile?.name ?? "OpenAI"} · {model}</p>
          </div>
          <div className="topbarActions">
            <select value={activeProfileId} onChange={(event) => setActiveProfileId(event.target.value)}>
              {profiles.map((profile) => (
                <option key={profile.id} value={profile.id}>
                  {profile.name}
                </option>
              ))}
            </select>
            <button className="secondaryButton" onClick={saveProfile} disabled={isSavingProfile}>
              {isSavingProfile ? "Saving" : "Save settings"}
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
            baseUrl={baseUrl}
            setBaseUrl={setBaseUrl}
            model={model}
            setModel={setModel}
            apiKey={apiKey}
            setApiKey={setApiKey}
            saveApiKey={saveApiKey}
            setSaveApiKey={setSaveApiKey}
            activeProfile={activeProfile}
          />
        ) : activeView === "history" ? (
          <GalleryHistoryView
            history={history}
            query={historyQuery}
            thumbnailUrls={thumbnailUrls}
            selectedId={selected?.generation.id}
            onQuery={onHistorySearch}
            onSelect={selectGeneration}
            onUse={useGeneration}
            onReveal={revealGeneration}
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

              <div className="controlGrid">
                <Field label="Model">
                  <input value={model} onChange={(event) => setModel(event.target.value)} />
                </Field>
                <Field label="Size">
                  <select value={size} onChange={(event) => setSize(event.target.value)}>
                    {sizes.map((item) => (
                      <option key={item} value={item}>
                        {item}
                      </option>
                    ))}
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
                  {isGenerating ? "Generating" : "Generate image"}
                </button>
                <span>{selectedSize} · {quality} · {outputFormat}</span>
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
              onReveal={revealSelected}
              onDelete={deleteSelected}
            />
          </div>
        )}
      </section>
    </main>
  );
}

function GalleryHistoryView(props: {
  history: GenerationDetail[];
  query: string;
  thumbnailUrls: Record<string, string>;
  selectedId?: string;
  onQuery: (value: string) => void;
  onSelect: (detail: GenerationDetail) => void;
  onUse: (detail: GenerationDetail) => void;
  onReveal: (detail: GenerationDetail) => void;
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
              <button className="galleryPreview" onClick={() => props.onSelect(detail)}>
                {thumbnail ? (
                  <img src={thumbnail} alt="Generated output thumbnail" />
                ) : (
                  <span className={`galleryPlaceholder ${detail.generation.status}`}>
                    {detail.generation.status}
                  </span>
                )}
              </button>
              <div className="galleryMeta">
                <p className="galleryPrompt">{detail.generation.prompt || "Untitled prompt"}</p>
                <div className="galleryStats">
                  <span>{detail.generation.model}</span>
                  <span>{detail.generation.size}</span>
                  <span>{formatTime(detail.generation.createdAt)}</span>
                </div>
                {detail.generation.errorMessage && (
                  <p className="galleryError">{detail.generation.errorMessage}</p>
                )}
                <div className="galleryActions">
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
  baseUrl: string;
  setBaseUrl: (value: string) => void;
  model: string;
  setModel: (value: string) => void;
  apiKey: string;
  setApiKey: (value: string) => void;
  saveApiKey: boolean;
  setSaveApiKey: (value: boolean) => void;
  activeProfile?: ProviderProfile;
}) {
  return (
    <section className="settingsPane">
      <div className="settingsGroup">
        <Field label="Base URL">
          <input value={props.baseUrl} onChange={(event) => props.setBaseUrl(event.target.value)} />
        </Field>
        <Field label="Default model">
          <input value={props.model} onChange={(event) => props.setModel(event.target.value)} />
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
        <h2>Provider slot</h2>
        <p>OpenAI-compatible image generation is active. Google image models can be added as another provider adapter without changing history records.</p>
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
              <small>{detail.generation.model} · {formatTime(detail.generation.createdAt)}</small>
            </span>
          </button>
        ))}
        {props.history.length === 0 && <div className="emptyState">No generations</div>}
      </div>
    </section>
  );
}

function Inspector(props: {
  detail: GenerationDetail | null;
  imageDataUrl: string;
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
          <dd>{props.detail?.generation.status ?? "idle"}</dd>
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

function errorMessage(err: unknown) {
  if (err instanceof Error) return err.message;
  return String(err);
}

function knownSize(value: string) {
  return sizes.includes(value);
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
