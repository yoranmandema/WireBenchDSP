// App boot: the UI must leave the "connecting" screen once the engine greets
// it, and must say so plainly when the engine cannot be reached at all.

describe("app boot", () => {
  it("connects to the engine and renders the rig", () => {
    cy.boot();

    cy.get(".topbar").should("contain", "01-clean");
    cy.get(".rack").should("exist");
    cy.get(".chain-panel").should("exist");
    cy.get(".editor-empty").should("contain", "Select a block");
    cy.get(".cpu-value").should("contain", "%");
  });

  it("shows the engine-unavailable screen when the socket never opens", () => {
    cy.visit("/", {
      onBeforeLoad(win) {
        // A socket that neither opens nor closes leaves the bridge's request
        // queued until it times out with "engine unreachable". (A socket that
        // closes is treated as a reconnect, not a failure.)
        class DeadSocket {
          readyState = 0; // CONNECTING, forever
          send() {}
          close() {}
        }
        win.WebSocket = DeadSocket;
      },
    });

    cy.contains("Engine unavailable", { timeout: 15000 }).should("be.visible");
    cy.contains("Start the engine").should("exist");
  });
});
