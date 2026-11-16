// The signal chain: channel rows, block cards, selection, bypass, remove,
// volume, sources, and the two drag interactions.

describe("signal chain", () => {
  beforeEach(() => {
    cy.boot();
  });

  it("renders four channels with their names", () => {
    cy.get(".channel").should("have.length", 4);
    cy.get(".channel-name")
      .then(($n) => [...$n].map((el) => el.textContent.trim()))
      .should("deep.equal", ["Channel 1", "Channel 2", "Channel 3", "Channel 4"]);
  });

  it("shows an empty-channel hint", () => {
    cy.get(".channel").eq(2).find(".strip-empty").should("contain", "Empty channel");
    cy.get(".channel").eq(2).find(".block-card").should("have.length", 0);
  });

  it("selects a block and shows it in the editor", () => {
    cy.selectBlock(0, 0);
    cy.get(".block-card.selected").should("have.length", 1);
    cy.get(".editor-name").should("contain", "Amp");
    cy.get(".editor-type").should("contain", "amp.basic");
  });

  it("toggles bypass from the card", () => {
    cy.selectBlock(0, 0);
    cy.get(".channel").eq(0).find(".block-card").eq(0).find(".bypass").click();

    cy.get(".channel").eq(0).find(".block-card").eq(0).should("have.class", "bypass");
    cy.get(".editor-bypass").should("contain", "bypassed");
    cy.get(".side-btn").contains("Bypassed").should("exist");
  });

  it("removes a block", () => {
    cy.get(".channel").eq(0).find(".block-card").should("have.length", 3);

    cy.get(".channel").eq(0).find(".block-card").eq(1).find(".remove").click();

    cy.get(".channel").eq(0).find(".block-card").should("have.length", 2);
    cy.get(".channel").eq(0).find(".block-card").should("not.contain", "Delay");
  });

  it("shows a toast when the engine rejects a removal", () => {
    // The seeded `locked` block always refuses `removeBlock`.
    cy.get(".channel").eq(0).find(".block-card").eq(2).find(".remove").click();

    cy.get(".toast").should("contain", "no such block");
    cy.get(".toast").click();
    cy.get(".toast").should("not.exist");
    cy.get(".channel").eq(0).find(".block-card").should("have.length", 3);
  });

  it("changes the channel volume", () => {
    cy.get(".channel").eq(0).find('input[type="range"]')
      .invoke("val", "-12")
      .trigger("input");

    cy.get(".channel").eq(0).find('input[type="range"]').should("have.value", "-12");
    cy.get(".channel").eq(0).find('input[type="range"]').should("have.attr", "title", "Volume -12.0 dB");
  });

  it("toggles the channel input source", () => {
    const box = () => cy.get(".channel").eq(0).find(".mini input[type='checkbox']");
    box().should("be.checked");
    box().uncheck();
    box().should("not.be.checked");
    box().check();
    box().should("be.checked");
  });

  it("reorders a block by dragging it to another slot", () => {
    const cards = () => cy.get(".channel").eq(0).find(".block-card");
    cards().eq(0).find(".block-name").should("have.text", "Amp");

    cy.get(".channel").eq(0).find(".strip").then(($strip) => {
      const r = $strip[0].getBoundingClientRect();
      const y = Math.round(r.top + r.height / 2);
      const startX = Math.round(r.left + 40);
      const endX = Math.round(r.right - 5);

      cards().eq(0).trigger("pointerdown", { pointerId: 1, button: 0, clientX: startX, clientY: y });
      cards().eq(0).trigger("pointermove", { pointerId: 1, clientX: endX, clientY: y });
      cards().eq(0).trigger("pointerup", { pointerId: 1 });
    });

    cards().eq(0).find(".block-name").should("have.text", "Delay");
    cards().eq(2).find(".block-name").should("have.text", "Amp");
  });

  it("inserts a block dragged from the rack into a channel", () => {
    cy.get(".channel").eq(2).find(".block-card").should("have.length", 0);

    cy.window().then((win) => {
      const dt = new win.DataTransfer();
      cy.contains(".rack-item", /^Reverb$/).then(($row) => {
        const row = $row[0];
        row.dispatchEvent(new win.DragEvent("dragstart", { bubbles: true, cancelable: true, dataTransfer: dt }));
        cy.get(".channel").eq(2).then(($ch) => {
          const ch = $ch[0];
          ch.dispatchEvent(new win.DragEvent("dragenter", { bubbles: true, cancelable: true, dataTransfer: dt }));
          ch.dispatchEvent(new win.DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: dt }));
          ch.dispatchEvent(new win.DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: dt }));
          row.dispatchEvent(new win.DragEvent("dragend", { bubbles: true, cancelable: true, dataTransfer: dt }));
        });
      });
    });

    cy.get(".channel").eq(2).find(".block-card").should("have.length", 1);
    cy.get(".channel").eq(2).find(".block-card").should("contain", "Reverb");
  });
});

// Empty channels: they are fixed slots, so they stay in the layout and keep
// their height, they render none of the per-block machinery, and dropping a
// block into one resumes normal processing on the same snapshot.
describe("empty channels", () => {
  beforeEach(() => {
    cy.boot();
  });

  it("stays visible and keeps the height of a populated row", () => {
    cy.get(".channel").should("have.length", 4);
    cy.get(".channel").eq(2).should("have.class", "is-empty");
    cy.get(".channel").eq(2).find(".channel-name").should("have.text", "Channel 3");
    cy.get(".channel").eq(2).find(".strip-empty").should("contain", "Empty channel");

    // Same strip height as a row with cards, so the rows below do not jump.
    cy.get(".channel").eq(0).find(".strip").then(($populated) => {
      const h = $populated[0].getBoundingClientRect().height;
      cy.get(".channel").eq(2).find(".strip").then(($empty) => {
        expect($empty[0].getBoundingClientRect().height).to.be.closeTo(h, 1);
      });
    });
  });

  it("has no cards, drag state or drop indicator to pay for", () => {
    cy.get(".channel").eq(2).find(".block-card").should("have.length", 0);
    cy.get(".channel").eq(2).find(".block-placeholder").should("have.length", 0);
    cy.get(".channel").eq(2).find(".drop-indicator").should("have.length", 0);
    cy.get(".channel").eq(2).find(".drag-ghost").should("have.length", 0);
    // The header is still a working control on an empty channel.
    cy.get(".channel").eq(2).find(".channel-head input[type=range]").should("exist");
    cy.get(".channel").eq(2).find(".add-block").should("exist");
  });

  it("resumes normal processing when a block is added, without a reload", () => {
    cy.get(".channel").eq(2).find(".add-block").click();
    cy.contains(".picker .rack-item", /^Overdrive$/).click();

    cy.get(".modal.picker").should("not.exist");
    cy.get(".channel").eq(2).should("not.have.class", "is-empty");
    cy.get(".channel").eq(2).find(".block-card").should("have.length", 1);
    cy.get(".channel").eq(2).find(".strip-empty").should("have.length", 0);

    // And the block is immediately usable: selectable, editable, draggable.
    cy.selectBlock(2, 0);
    cy.get(".editor-name").should("contain", "Overdrive");
    cy.get(".channel").eq(2).find(".block-card").eq(0).find(".remove").click();
    cy.get(".channel").eq(2).find(".block-card").should("have.length", 0);
    cy.get(".channel").eq(2).should("have.class", "is-empty");
    cy.get(".channel").should("have.length", 4);
  });

  it("keeps its volume and source controls working while empty", () => {
    cy.get(".channel").eq(3).find(".channel-head input[type=range]")
      .invoke("val", "-12")
      .trigger("input");
    cy.get(".toast").should("not.exist");

    // The source toggle is live on an empty channel too: ticking it sends and
    // the engine's snapshot comes back with the channel still present.
    cy.get(".channel").eq(3).find(".mini input[type=checkbox]").should("not.be.checked").check();
    cy.get(".channel").eq(3).find(".mini input[type=checkbox]").should("be.checked");
    cy.get(".channel").should("have.length", 4);
  });
});
