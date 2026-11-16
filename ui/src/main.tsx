import { render } from "preact";
import { App } from "./App";
import "./styles.css";
import "./responsive.css";
import "./layout.css";

const root = document.getElementById("app");
if (root) render(<App />, root);
