import { createRoot } from "react-dom/client";
import "../../i18n";
import { applicationKindFromQuery, syncDocumentAppKind } from "../../lib/types";
import "../../styles/global.css";
import { HomePage } from "./HomePage";

syncDocumentAppKind(applicationKindFromQuery());
createRoot(document.getElementById("root")!).render(<HomePage />);
