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

  it("inserts a dragged block into a populated channel at the pointer slot", () => {
    const cards = () => cy.get(".channel").eq(0).find(".block-card");
    cards().eq(0).find(".block-name").should("have.text", "Amp");
    cards().eq(1).find(".block-name").should("have.text", "Delay");

    cy.window().then((win) => {
      const dt = new win.DataTransfer();
      cy.contains(".rack-item", /^Reverb$/).then(($row) => {
        const row = $row[0];
        row.dispatchEvent(new win.DragEvent("dragstart", { bubbles: true, cancelable: true, dataTransfer: dt }));
        cards().eq(0).then(($a) => {
          cards().eq(1).then(($b) => {
            const ra = $a[0].getBoundingClientRect();
            const rb = $b[0].getBoundingClientRect();
            // Halfway across the gap between Amp and Delay: slot 1.
            const x = Math.round((ra.right + rb.left) / 2);
            const y = Math.round(ra.top + ra.height / 2);
            cy.get(".channel").eq(0).then(($ch) => {
              const ch = $ch[0];
              ch.dispatchEvent(new win.DragEvent("dragenter", { bubbles: true, cancelable: true, dataTransfer: dt, clientX: x, clientY: y }));
              ch.dispatchEvent(new win.DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: dt, clientX: x, clientY: y }));
              ch.dispatchEvent(new win.DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: dt, clientX: x, clientY: y }));
              row.dispatchEvent(new win.DragEvent("dragend", { bubbles: true, cancelable: true, dataTransfer: dt }));
            });
          });
        });
      });
    });

    // Exactly one block lands, at the requested slot.
    cards().should("have.length", 4);
    cards().eq(0).find(".block-name").should("have.text", "Amp");
    cards().eq(1).find(".block-name").should("have.text", "Reverb");
    cards().eq(2).find(".block-name").should("have.text", "Delay");
  });

  it("appends a dragged block dropped past the last card", () => {
    const cards = () => cy.get(".channel").eq(0).find(".block-card");

    cy.window().then((win) => {
      const dt = new win.DataTransfer();
      cy.contains(".rack-item", /^Reverb$/).then(($row) => {
        const row = $row[0];
        row.dispatchEvent(new win.DragEvent("dragstart", { bubbles: true, cancelable: true, dataTransfer: dt }));
        cards().last().then(($last) => {
          const r = $last[0].getBoundingClientRect();
          const x = Math.round(r.right + 12);
          const y = Math.round(r.top + r.height / 2);
          cy.get(".channel").eq(0).then(($ch) => {
            const ch = $ch[0];
            ch.dispatchEvent(new win.DragEvent("dragenter", { bubbles: true, cancelable: true, dataTransfer: dt, clientX: x, clientY: y }));
            ch.dispatchEvent(new win.DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: dt, clientX: x, clientY: y }));
            ch.dispatchEvent(new win.DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: dt, clientX: x, clientY: y }));
            row.dispatchEvent(new win.DragEvent("dragend", { bubbles: true, cancelable: true, dataTransfer: dt }));
          });
        });
      });
    });

    cards().should("have.length", 4);
    cards().eq(0).find(".block-name").should("have.text", "Amp");
    cards().last().find(".block-name").should("have.text", "Reverb");
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

  it("shows the active drop highlight on an empty channel and clears it on leave", () => {
    let baseBorder;
    let baseBackground;
    const dt = new DataTransfer();
    dt.setData("application/x-block-type", JSON.stringify({ type: "reverb", name: "Reverb" }));
    cy.get(".channel").eq(2).then(($channel) => {
      const channel = $channel[0];
      const before = getComputedStyle(channel);
      baseBorder = before.borderTopColor;
      baseBackground = before.backgroundColor;
      channel.dispatchEvent(new DragEvent("dragenter", { bubbles: true, cancelable: true, dataTransfer: dt }));
      channel.dispatchEvent(new DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: dt }));
    });
    cy.get(".channel").eq(2).should("have.class", "drop-target-active")
      // The highlight is a 120 ms transition, so read it inside `should`:
      // Cypress retries until it has actually painted instead of catching the
      // first frame, which still carries the base colour.
      .should(($channel) => {
        const active = getComputedStyle($channel[0]);
        expect(active.borderTopColor).not.to.equal(baseBorder);
        expect(active.backgroundColor).not.to.equal(baseBackground);
        expect(active.boxShadow).not.to.equal("none");
      })
      .then(($channel) => {
        $channel[0].dispatchEvent(new DragEvent("dragleave", { bubbles: true, cancelable: true, dataTransfer: dt, relatedTarget: document.body }));
      });
    cy.get(".channel").eq(2).should("not.have.class", "drop-target-active");
  });

  it("shows the active drop highlight on a populated channel", () => {
    let baseBorder;
    let baseBackground;
    cy.get(".channel").eq(0).then(($channel) => {
      const channel = $channel[0];
      const before = getComputedStyle(channel);
      baseBorder = before.borderTopColor;
      baseBackground = before.backgroundColor;
      const dt = new DataTransfer();
      dt.setData("application/x-block-type", JSON.stringify({ type: "reverb", name: "Reverb" }));
      channel.dispatchEvent(new DragEvent("dragenter", { bubbles: true, cancelable: true, dataTransfer: dt }));
      channel.dispatchEvent(new DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: dt }));
    });
    cy.get(".channel").eq(0).should("have.class", "drop-target-active")
      // Retry through the 120 ms highlight transition; see the empty-channel
      // test above.
      .should(($channel) => {
        const active = getComputedStyle($channel[0]);
        expect(active.borderTopColor).not.to.equal(baseBorder);
        expect(active.backgroundColor).not.to.equal(baseBackground);
        expect(active.boxShadow).not.to.equal("none");
      });
  });

  it("has no cards, drag state or drop indicator to pay for", () => {
    cy.get(".channel").eq(2).find(".block-card").should("have.length", 0);
    cy.get(".channel").eq(2).find(".block-placeholder").should("have.length", 0);
    cy.get(".channel").eq(2).find(".drop-indicator").should("have.length", 0);
    cy.get(".channel").eq(2).find(".drag-ghost").should("have.length", 0);
    // The header is still a working control on an empty channel.
    cy.get(".channel").eq(2).find(".channel-head input").should("have.length", 0);
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
});
