import { createRoot } from "react-dom/client";
import "../../i18n";
import "../../styles/global.css";
import { HomePage } from "./HomePage";

createRoot(document.getElementById("root")!).render(<HomePage />);
