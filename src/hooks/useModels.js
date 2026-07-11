import { useCallback, useEffect, useMemo, useState, } from "react";
import { deleteOllamaModel, listOllamaModels, pullOllamaModel, runOllamaModel, showOllamaModel, } from "../services/models";
function useModels({ refreshInterval, onMessage, }) {
    const [models, setModels,] = useState([]);
    const [status, setStatus,] = useState("idle");
    const [activeModel, setActiveModel,] = useState(null);
    const [pullProgress, setPullProgress,] = useState(null);
    const [error, setError,] = useState("");
    const [searchText, setSearchText,] = useState("");
    const refreshModels = useCallback(async () => {
        try {
            setStatus("loading");
            setError("");
            const result = await listOllamaModels();
            setModels(result);
            setStatus("idle");
        }
        catch (nextError) {
            const message = `Unable to load Ollama models: ${String(nextError)}`;
            setStatus("error");
            setError(message);
        }
    }, []);
    useEffect(() => {
        refreshModels();
    }, [refreshModels]);
    useEffect(() => {
        const interval = window.setInterval(() => {
            if (status === "idle" ||
                status === "error") {
                refreshModels();
            }
        }, Math.max(refreshInterval, 5) * 1000);
        return () => {
            window.clearInterval(interval);
        };
    }, [
        refreshInterval,
        refreshModels,
        status,
    ]);
    const pullModel = useCallback(async (model) => {
        const normalized = model.trim();
        if (!normalized) {
            const message = "Enter a model name first.";
            setError(message);
            onMessage(`❌ ${message}`);
            return;
        }
        try {
            setStatus("pulling");
            setActiveModel(normalized);
            setPullProgress({
                status: "Preparing download...",
            });
            setError("");
            const result = await pullOllamaModel(normalized);
            setPullProgress(result);
            onMessage(`✅ Model ${normalized} downloaded successfully.`);
            await refreshModels();
        }
        catch (nextError) {
            const message = `Unable to download ${normalized}: ${String(nextError)}`;
            setStatus("error");
            setError(message);
            onMessage(`❌ ${message}`);
        }
        finally {
            setActiveModel(null);
            setPullProgress(null);
            setStatus((current) => current === "error"
                ? "error"
                : "idle");
        }
    }, [
        onMessage,
        refreshModels,
    ]);
    const removeModel = useCallback(async (model) => {
        try {
            setStatus("deleting");
            setActiveModel(model);
            setError("");
            const result = await deleteOllamaModel(model);
            onMessage(`🗑️ ${result}`);
            await refreshModels();
        }
        catch (nextError) {
            const message = `Unable to delete ${model}: ${String(nextError)}`;
            setStatus("error");
            setError(message);
            onMessage(`❌ ${message}`);
        }
        finally {
            setActiveModel(null);
            setStatus((current) => current === "error"
                ? "error"
                : "idle");
        }
    }, [
        onMessage,
        refreshModels,
    ]);
    const testModel = useCallback(async (model, prompt) => {
        const normalizedPrompt = prompt.trim();
        if (!normalizedPrompt) {
            throw new Error("Enter a test prompt first.");
        }
        try {
            setStatus("running");
            setActiveModel(model);
            setError("");
            const result = await runOllamaModel(model, normalizedPrompt);
            onMessage(`✅ ${model} completed the test prompt.`);
            return result;
        }
        catch (nextError) {
            const message = `Unable to run ${model}: ${String(nextError)}`;
            setStatus("error");
            setError(message);
            throw new Error(message);
        }
        finally {
            setActiveModel(null);
            setStatus((current) => current === "error"
                ? "error"
                : "idle");
        }
    }, [onMessage]);
    const inspectModel = useCallback(async (model) => {
        try {
            setStatus("loading");
            setActiveModel(model);
            setError("");
            return await showOllamaModel(model);
        }
        catch (nextError) {
            const message = `Unable to inspect ${model}: ${String(nextError)}`;
            setStatus("error");
            setError(message);
            throw new Error(message);
        }
        finally {
            setActiveModel(null);
            setStatus((current) => current === "error"
                ? "error"
                : "idle");
        }
    }, []);
    const filteredModels = useMemo(() => {
        const search = searchText
            .trim()
            .toLowerCase();
        if (!search) {
            return models;
        }
        return models.filter((model) => model.name
            .toLowerCase()
            .includes(search) ||
            model.model
                .toLowerCase()
                .includes(search) ||
            model.details?.family
                ?.toLowerCase()
                .includes(search) ||
            model.details
                ?.parameterSize
                ?.toLowerCase()
                .includes(search));
    }, [
        models,
        searchText,
    ]);
    const totalSize = useMemo(() => models.reduce((total, model) => total +
        model.size, 0), [models]);
    return {
        models,
        filteredModels,
        totalSize,
        status,
        activeModel,
        pullProgress,
        error,
        searchText,
        setSearchText,
        refreshModels,
        pullModel,
        removeModel,
        testModel,
        inspectModel,
    };
}
export default useModels;
