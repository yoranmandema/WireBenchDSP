// Preset navigation (top bar) and the preset browser modal.

describe("preset bar", () => {
  beforeEach(() => {
    cy.boot();
  });

  it("shows the current preset, its source, and the active slot", () => {
    cy.get(".preset-name").should("contain", "01-clean");
    cy.get(".preset-name small").should("have.text", "riff");
    cy.get(".slot.on").should("have.length", 1);
    cy.get(".slot.on").should("have.text", "B");
    cy.get(".chip.on").should("have.text", "riff");
  });

  it("steps to the next and previous preset", () => {
    cy.get('button[title="Next preset"]').click();
    cy.get(".preset-name").should("contain", "02-lead");

    cy.get('button[title="Previous preset"]').click();
    cy.get(".preset-name").should("contain", "01-clean");

    cy.get('button[title="Previous preset"]').click();
    cy.get(".preset-name").should("contain", "00-blank");

    // The first preset has no predecessor: the button is a no-op.
    cy.get('button[title="Previous preset"]').click();
    cy.get(".preset-name").should("contain", "00-blank");
  });

  it("loads a preset from its slot", () => {
    cy.get(".slot").eq(2).click();
    cy.get(".preset-name").should("contain", "02-lead");
    cy.get(".slot.on").should("have.text", "C");
  });

  it("switches the input source", () => {
    cy.get(".chip").contains("live").click();
    cy.get(".chip.on").should("have.text", "live");
    cy.get(".preset-name small").should("have.text", "live");

    cy.get(".chip").contains("silence").click();
    cy.get(".chip.on").should("have.text", "silence");
    cy.get(".preset-name small").should("have.text", "silence");
  });
});

describe("preset browser", () => {
  beforeEach(() => {
    cy.boot();
    cy.openPresetBrowser();
  });

  it("lists the presets and marks the current one", () => {
    cy.get(".preset-load").should("have.length", 3);
    cy.get(".preset-row.current .preset-load").should("contain", "01-clean");
    cy.get(".preset-row").eq(0).should("not.have.class", "current");
  });

  it("loads a preset and closes", () => {
    cy.get(".preset-load").contains("02-lead").click();
    cy.get(".preset-name").should("contain", "02-lead");

    cy.get(".modal-head .nav").click();
    cy.get(".modal").should("not.exist");
  });

  it("saves the current rig under a new name", () => {
    cy.get(".preset-new").type("03-test");
    cy.get(".modal-foot .btn").should("not.be.disabled").click();

    cy.get(".preset-load").should("have.length", 4);
    cy.get(".preset-row.current .preset-load").should("contain", "03-test");
  });

  it("refuses a duplicate name", () => {
    cy.get(".preset-new").type("01-clean");
    cy.get(".modal-foot .btn").should("be.disabled");
    cy.get(".modal-hint").should("contain", "already exists");
  });

  it("renames a preset", () => {
    cy.get(".preset-row").eq(0).contains("Rename").click();
    cy.get(".preset-rename").should("have.value", "00-blank").clear().type("00-empty{enter}");

    cy.get(".preset-load").should("contain", "00-empty");
    cy.get(".preset-load").should("not.contain", "00-blank");
  });

  it("deletes a preset after confirming", () => {
    cy.get(".preset-load").should("have.length", 3);
    cy.get(".preset-row").eq(0).contains("Delete").click();

    cy.get(".preset-load").should("have.length", 2);
    cy.get(".preset-load").should("not.contain", "00-blank");
  });

  it("surfaces a refused delete as a toast", () => {
    // `mutate` in state.ts turns a bridge rejection into the global toast, so
    // the modal's own `.modal-error` branch is not reachable from the UI.
    cy.get(".preset-row").eq(2).contains("Delete").click();

    cy.get(".toast").should("contain", "preset is locked");
    cy.get(".preset-load").should("have.length", 3);
  });
});
