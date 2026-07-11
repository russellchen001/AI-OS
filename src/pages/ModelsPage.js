import { jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment } from "react/jsx-runtime";
import { useMemo, useState, } from "react";
import { POPULAR_OLLAMA_MODELS, } from "../config/constants";
function formatBytes(bytes) {
    if (!Number.isFinite(bytes) ||
        bytes <= 0) {
        return "0 B";
    }
    const units = [
        "B",
        "KB",
        "MB",
        "GB",
        "TB",
    ];
    const index = Math.min(Math.floor(Math.log(bytes) /
        Math.log(1024)), units.length - 1);
    const value = bytes /
        1024 ** index;
    return `${value.toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}
function formatDate(value) {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) {
        return value;
    }
    return date.toLocaleString();
}
function progressPercent(progress) {
    if (!progress?.total ||
        !progress.completed) {
        return 0;
    }
    return Math.min(Math.max((progress.completed /
        progress.total) *
        100, 0), 100);
}
function ModelsPage({ models, totalSize, status, activeModel, pullProgress, error, searchText, cardStyle, onSearchChange, onRefresh, onPull, onDelete, onTest, onInspect, }) {
    const [modelInput, setModelInput,] = useState("");
    const [confirmDelete, setConfirmDelete,] = useState(null);
    const [selectedModel, setSelectedModel,] = useState(null);
    const [testPrompt, setTestPrompt,] = useState("Reply with one short sentence confirming that the model is working.");
    const [testResult, setTestResult,] = useState("");
    const [modelDetails, setModelDetails,] = useState("");
    const [modalError, setModalError,] = useState("");
    const isBusy = status === "loading" ||
        status === "pulling" ||
        status === "deleting" ||
        status === "running";
    const selectedModelRecord = useMemo(() => models.find((model) => model.name ===
        selectedModel) ?? null, [
        models,
        selectedModel,
    ]);
    const percent = progressPercent(pullProgress);
    async function openModel(model) {
        setSelectedModel(model.name);
        setTestResult("");
        setModelDetails("");
        setModalError("");
        try {
            const details = await onInspect(model.name);
            setModelDetails(details);
        }
        catch (nextError) {
            setModalError(String(nextError));
        }
    }
    async function runTest() {
        if (!selectedModel) {
            return;
        }
        try {
            setTestResult("");
            setModalError("");
            const result = await onTest(selectedModel, testPrompt);
            setTestResult(result);
        }
        catch (nextError) {
            setModalError(String(nextError));
        }
    }
    return (_jsxs("section", { className: "page-section", children: [_jsxs("div", { className: "section-header", children: [_jsxs("div", { children: [_jsx("h2", { children: "Ollama Models" }), _jsx("p", { children: "Download, inspect, test and remove local AI models." })] }), _jsx("button", { type: "button", className: "secondary-button", disabled: isBusy, onClick: onRefresh, children: status === "loading"
                            ? "Refreshing..."
                            : "↻ Refresh" })] }), _jsxs("div", { className: "models-summary-grid", children: [_jsxs("div", { className: "models-summary-card", style: cardStyle, children: [_jsx("span", { children: "Installed Models" }), _jsx("strong", { children: models.length })] }), _jsxs("div", { className: "models-summary-card", style: cardStyle, children: [_jsx("span", { children: "Storage Used" }), _jsx("strong", { children: formatBytes(totalSize) })] }), _jsxs("div", { className: "models-summary-card", style: cardStyle, children: [_jsx("span", { children: "Runtime" }), _jsx("strong", { children: "Ollama" })] })] }), _jsxs("div", { className: "models-download-card", style: cardStyle, children: [_jsxs("div", { className: "models-download-heading", children: [_jsxs("div", { children: [_jsx("h3", { children: "Download Model" }), _jsx("p", { children: "Enter an Ollama model tag, such as llama3.2:3b." })] }), _jsx("span", { children: "\u2B07\uFE0F" })] }), _jsxs("div", { className: "models-download-form", children: [_jsx("input", { type: "text", value: modelInput, disabled: isBusy, placeholder: "llama3.2:3b", onChange: (event) => setModelInput(event.target.value), onKeyDown: (event) => {
                                    if (event.key ===
                                        "Enter") {
                                        onPull(modelInput);
                                    }
                                } }), _jsx("button", { type: "button", className: "action-button backup-button", disabled: isBusy ||
                                    !modelInput.trim(), onClick: () => onPull(modelInput), children: status === "pulling"
                                    ? "Downloading..."
                                    : "Download" })] }), _jsxs("div", { className: "popular-models", children: [_jsx("span", { children: "Popular:" }), POPULAR_OLLAMA_MODELS.map((model) => (_jsx("button", { type: "button", className: "model-suggestion", disabled: isBusy, title: model.description, onClick: () => setModelInput(model.name), children: model.name }, model.name)))] }), status === "pulling" &&
                        pullProgress && (_jsxs("div", { className: "model-download-progress", children: [_jsxs("div", { className: "model-progress-header", children: [_jsx("span", { children: pullProgress.status }), percent > 0 && (_jsxs("strong", { children: [percent.toFixed(1), "%"] }))] }), _jsx("div", { className: "metric-track", children: _jsx("div", { className: "metric-progress", style: {
                                        width: percent > 0
                                            ? `${percent}%`
                                            : "8%",
                                        background: "#3b82f6",
                                    } }) })] })), error && (_jsx("div", { className: "models-error", role: "alert", children: error }))] }), _jsxs("div", { className: "section-header models-list-header", children: [_jsxs("div", { children: [_jsx("h2", { children: "Installed Models" }), _jsx("p", { children: "Models currently available to Ollama." })] }), _jsx("input", { type: "search", className: "models-search", value: searchText, placeholder: "Search models...", onChange: (event) => onSearchChange(event.target.value) })] }), models.length === 0 ? (_jsxs("div", { className: "models-empty-state", style: cardStyle, children: [_jsx("span", { children: "\uD83E\uDDE0" }), _jsx("h3", { children: "No models installed" }), _jsx("p", { children: "Download a model to begin using local AI." })] })) : (_jsx("div", { className: "models-grid", children: models.map((model) => {
                    const busy = activeModel ===
                        model.name;
                    const deleting = confirmDelete ===
                        model.name;
                    return (_jsxs("article", { className: "model-card", style: cardStyle, children: [_jsxs("div", { className: "model-card-header", children: [_jsx("span", { className: "model-card-icon", children: "\uD83E\uDDE0" }), _jsxs("div", { children: [_jsx("h3", { children: model.name }), _jsx("p", { children: model.details
                                                    ?.family ??
                                                    "Ollama model" })] })] }), _jsxs("div", { className: "model-meta-grid", children: [_jsxs("div", { children: [_jsx("span", { children: "Size" }), _jsx("strong", { children: formatBytes(model.size) })] }), _jsxs("div", { children: [_jsx("span", { children: "Parameters" }), _jsx("strong", { children: model.details
                                                    ?.parameterSize ??
                                                    "Unknown" })] }), _jsxs("div", { children: [_jsx("span", { children: "Quantization" }), _jsx("strong", { children: model.details
                                                    ?.quantizationLevel ??
                                                    "Unknown" })] })] }), _jsxs("small", { className: "model-modified", children: ["Updated", " ", formatDate(model.modifiedAt)] }), _jsxs("div", { className: "model-card-actions", children: [_jsx("button", { type: "button", className: "action-button health-button", disabled: isBusy, onClick: () => openModel(model), children: busy &&
                                            status ===
                                                "loading"
                                            ? "Opening..."
                                            : "Test" }), deleting ? (_jsxs(_Fragment, { children: [_jsx("button", { type: "button", className: "danger-button", disabled: busy, onClick: () => {
                                                    onDelete(model.name);
                                                    setConfirmDelete(null);
                                                }, children: "Confirm" }), _jsx("button", { type: "button", className: "secondary-button", onClick: () => setConfirmDelete(null), children: "Cancel" })] })) : (_jsx("button", { type: "button", className: "danger-button", disabled: isBusy, onClick: () => setConfirmDelete(model.name), children: "Delete" }))] })] }, model.name));
                }) })), selectedModelRecord && (_jsx("div", { className: "model-modal-backdrop", role: "presentation", onMouseDown: (event) => {
                    if (event.target ===
                        event.currentTarget) {
                        setSelectedModel(null);
                    }
                }, children: _jsxs("div", { className: "model-modal", style: cardStyle, role: "dialog", "aria-modal": "true", "aria-label": `Test ${selectedModelRecord.name}`, children: [_jsxs("div", { className: "model-modal-header", children: [_jsxs("div", { children: [_jsx("h3", { children: selectedModelRecord.name }), _jsx("p", { children: "Test this model with a local prompt." })] }), _jsx("button", { type: "button", className: "secondary-button", disabled: status ===
                                        "running", onClick: () => setSelectedModel(null), children: "Close" })] }), _jsxs("label", { className: "setting-field", children: [_jsx("span", { children: "Test Prompt" }), _jsx("textarea", { value: testPrompt, disabled: status ===
                                        "running", rows: 5, onChange: (event) => setTestPrompt(event.target
                                        .value) })] }), _jsx("button", { type: "button", className: "action-button health-button", disabled: status ===
                                "running" ||
                                !testPrompt.trim(), onClick: runTest, children: status === "running"
                                ? "Running..."
                                : "Run Test" }), modelDetails && (_jsxs("details", { className: "model-details", children: [_jsx("summary", { children: "Model Details" }), _jsx("pre", { children: modelDetails })] })), testResult && (_jsxs("div", { className: "model-test-result", children: [_jsx("strong", { children: "Response" }), _jsx("pre", { children: testResult })] })), modalError && (_jsx("div", { className: "models-error", role: "alert", children: modalError }))] }) }))] }));
}
export default ModelsPage;
