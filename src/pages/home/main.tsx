import { createRoot } from "react-dom/client";
import "../../i18n";
import { mountStartupUpdateDialog } from "../../components/StartupUpdateDialog";
import { applicationKindFromQuery, syncDocumentAppKind } from "../../lib/types";
import "../../styles/global.css";
import { HomePage } from "./HomePage";

syncDocumentAppKind(applicationKindFromQuery());
mountStartupUpdateDialog();
createRoot(document.getElementById("root")!).render(<HomePage />);
