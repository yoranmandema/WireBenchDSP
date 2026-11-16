import { render } from "preact";
import { App } from "./App";
import "./styles.css";
import "./layout.css";
import "./responsive.css";

const root = document.getElementById("app");
if (root) render(<App />, root);
