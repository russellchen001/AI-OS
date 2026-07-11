import { jsx as _jsx } from "react/jsx-runtime";
import { StrictMode, } from "react";
import { createRoot, } from "react-dom/client";
import App from "./App";
import "./App.css";
const rootElement = document.getElementById("root");
if (!rootElement) {
    throw new Error("Unable to find the React root element.");
}
createRoot(rootElement).render(_jsx(StrictMode, { children: _jsx(App, {}) }));
