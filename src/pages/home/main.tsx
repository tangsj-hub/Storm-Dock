import { createRoot } from "react-dom/client";
import "../../i18n";
import { applicationKindFromQuery, homeModeFromQuery, syncDocumentAppKind } from "../../lib/types";
import "../../styles/global.css";
import { HomePage } from "./HomePage";

if (homeModeFromQuery() === "models") syncDocumentAppKind();
else syncDocumentAppKind(applicationKindFromQuery());
createRoot(document.getElementById("root")!).render(<HomePage />);
