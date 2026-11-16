// The add-block picker: the modal opened by a channel's "+" button.

describe("add-block picker", () => {
  beforeEach(() => {
    cy.boot();
  });

  it("opens with the search focused and names the target channel", () => {
    cy.get(".channel").eq(2).find(".add-block").click();

    cy.get(".modal.picker").should("be.visible");
    cy.get(".picker-channel").should("have.text", "Channel 3");
    cy.get(".picker-search").should("have.focus");
    cy.get(".picker-meta").first().should("contain", "11 blocks");
  });

  it("filters by name and adds the first match on Enter", () => {
    cy.get(".channel").eq(2).find(".add-block").click();
    cy.get(".picker-search").type("reverb");

    cy.get(".picker .rack-item").should("have.length", 1);
    cy.get(".picker .rack-item").should("contain", "Reverb");
    cy.get(".picker-meta").first().should("contain", "1 block");

    cy.get(".picker-search").type("{enter}");

    cy.get(".modal.picker").should("not.exist");
    cy.get(".channel").eq(2).find(".block-card").should("have.length", 1);
    cy.get(".channel").eq(2).find(".block-card").should("contain", "Reverb");
  });

  it("filters by block type as well as name", () => {
    cy.get(".channel").eq(2).find(".add-block").click();
    cy.get(".picker-search").type("time.delay");

    cy.get(".picker .rack-item").should("have.length", 1);
    cy.get(".picker .rack-item").should("contain", "Delay");
  });

  it("adds a block by clicking a row", () => {
    cy.get(".channel").eq(2).find(".add-block").click();
    cy.contains(".picker .rack-item", /^Overdrive$/).click();

    cy.get(".modal.picker").should("not.exist");
    cy.get(".channel").eq(2).find(".block-card").should("have.length", 1);
    cy.get(".channel").eq(2).find(".block-card").should("contain", "Overdrive");
  });

  it("shows an empty state when nothing matches", () => {
    cy.get(".channel").eq(2).find(".add-block").click();
    cy.get(".picker-search").type("zzz");

    cy.get(".picker-body .editor-empty").should("contain", "No blocks match");
    cy.get(".picker-meta").first().should("contain", "0 blocks");
  });

  it("closes on Escape", () => {
    cy.get(".channel").eq(0).find(".add-block").click();
    cy.get(".modal.picker").should("be.visible");

    cy.get(".picker-search").type("{esc}");
    cy.get(".modal.picker").should("not.exist");
  });

  it("closes on a backdrop click", () => {
    cy.get(".channel").eq(0).find(".add-block").click();
    cy.get(".modal.picker").should("be.visible");

    cy.get(".modal-backdrop").click("topLeft");
    cy.get(".modal.picker").should("not.exist");
  });

  it("closes on the × button", () => {
    cy.get(".channel").eq(0).find(".add-block").click();
    cy.get(".modal.picker").should("be.visible");

    cy.get(".modal.picker .modal-head .nav").click();
    cy.get(".modal.picker").should("not.exist");
  });

  it("resets the search and adds once per open across repeated cycles", () => {
    // First cycle: filter, then cancel.
    cy.get(".channel").eq(2).find(".add-block").click();
    cy.get(".picker-search").type("delay");
    cy.get(".picker-search").type("{esc}");
    cy.get(".modal.picker").should("not.exist");

    // Reopening starts from the full list, not the last filter.
    cy.get(".channel").eq(2).find(".add-block").click();
    cy.get(".picker-search").should("have.value", "");
    cy.get(".picker-meta").first().should("contain", "11 blocks");
    cy.contains(".picker .rack-item", /^Delay$/).click();
    cy.get(".modal.picker").should("not.exist");

    // Second cycle appends exactly one more block; nothing is duplicated
    // or dropped.
    cy.get(".channel").eq(2).find(".add-block").click();
    cy.contains(".picker .rack-item", /^Overdrive$/).click();
    cy.get(".modal.picker").should("not.exist");

    cy.get(".channel").eq(2).find(".block-card").should("have.length", 2);
    cy.get(".channel").eq(2).find(".block-name").then(($names) => {
      expect([...$names].map((n) => n.textContent.trim())).to.deep.equal([
        "Delay",
        "Overdrive",
      ]);
    });
  });
});
