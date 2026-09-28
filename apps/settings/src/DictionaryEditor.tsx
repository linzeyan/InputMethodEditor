// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  createTableColumn,
  Button,
  Input,
  makeStyles,
  TableColumnDefinition,
  OnSelectionChangeData,
  InputOnChangeData,
  shorthands,
  DataGridCell,
  useId,
  useToastController,
  Toast,
  ToastTitle,
  Toaster,
} from "@fluentui/react-components";
import {
  DataGrid,
  DataGridBody,
  DataGridHeader,
  DataGridHeaderCell,
  DataGridRow,
  RowRenderer,
} from "@fluentui-contrib/react-data-grid-react-window";
import { useEffect, useState, useRef } from "react";
import WordEditor from "./WordEditor";
import { invoke } from "@tauri-apps/api/core";
import { message } from "@tauri-apps/plugin-dialog";

const useStyles = makeStyles({
  root: {
    padding: "10px",
    display: "flex",
    "& .bopomofo": {
      fontFamily: "標楷體",
    },
  },
  leftPanel: {
    flex: 1,
    minWidth: "49%",
    display: "flex",
    flexDirection: "column",
    gap: "10px",
  },
  topPanel: {
    display: "flex",
    flexDirection: "row",
    gap: "5px",
  },
  bottomPanel: {
    marginTop: "20px",
    overflowX: "hidden",
    overflowY: "clip",
    flex: 1,
  },
  rightPanel: {
    flex: 1,
    minWidth: "50%",
    borderLeft: "1px solid #ccc",
    height: "95vh",
  },
  grid: {
    position: "absolute",
    "& > .fui-DataGridBody > div": {
      ...shorthands.overflow("clip !important", "auto"),
    },
  },
  search: {
    width: "90%",
  },
});

type DictionaryEntry = {
  word: string;
  bopomofo: string;
  boost: number;
};

type DictionaryEntryView = DictionaryEntry & {
  index: number;
};

const columns: TableColumnDefinition<DictionaryEntry>[] = [
  createTableColumn<DictionaryEntry>({
    columnId: "word",
    renderHeaderCell: () => <b>字/詞</b>,
    renderCell: (item) => item.word,
    compare: (a, b) => {
      return a.word.localeCompare(b.word);
    },
  }),
  createTableColumn<DictionaryEntry>({
    columnId: "reading",
    renderHeaderCell: () => <b>注音</b>,
    renderCell: (item) => <span className="bopomofo">{item.bopomofo}</span>,
    compare: (a, b) => {
      return a.bopomofo.localeCompare(b.bopomofo);
    },
  }),
  createTableColumn<DictionaryEntry>({
    columnId: "boost",
    renderHeaderCell: () => <b>偏好程度 (-100..100)</b>,
    renderCell: (item) => item.boost.toString(),
    compare: (a, b) => {
      return a.boost - b.boost;
    },
  }),
];

function DictionaryEditor() {
  const styles = useStyles();
  const [filter, setFilter] = useState<string>("");
  const [items, setItems] = useState<DictionaryEntry[]>([]);
  const [itemsView, setItemsView] = useState<DictionaryEntryView[]>([]);
  const [selected, setSelected] = useState<number>();
  const [gridHeight, setGridHeight] = useState<number>(400);

  const toasterId = useId("toaster");
  const { dispatchToast } = useToastController(toasterId);
  const saveComplete = () => {
    dispatchToast(
      <Toast>
        <ToastTitle>存檔成功</ToastTitle>
      </Toast>,
      { position: "bottom-end", intent: "success", timeout: 300 },
    );
  };

  const ref = useRef<HTMLDivElement | null>(null);

  const view = (items: DictionaryEntry[], filter: string) =>
    items
      .map((v, index) => ({ ...v, index }))
      .filter((v) => v.word.includes(filter));

  useEffect(() => {
    invoke("load")
      .then((value) => {
        const items = value as DictionaryEntry[];
        setItems(items);
        setItemsView(view(items, ""));
      })
      .catch((e) => {
        message(e, { title: "錯誤", kind: "error" });
      });
  }, []);

  const renderRow: RowRenderer<DictionaryEntry> = ({ item, rowId }, style) =>
    item.word.includes(filter) && (
      <DataGridRow key={rowId} style={style}>
        {({ renderCell }) => <DataGridCell>{renderCell(item)}</DataGridCell>}
      </DataGridRow>
    );

  const selectHandler = (_e: any, data: OnSelectionChangeData) => {
    const idx = data.selectedItems.values().next().value as number;
    setSelected(itemsView[idx].index);
  };

  const onInsert = () => {
    const nextItems = [...items];
    nextItems.push({
      word: "",
      bopomofo: "",
      boost: 0,
    });
    setItems(nextItems);
    setFilter("");
    setItemsView(view(nextItems, ""));
    setSelected(nextItems.length - 1);
  };

  const onDelete = () => {
    const nextItems = items.filter((_v, idx) => idx != selected);
    setItems(nextItems);
    setItemsView(view(nextItems, filter));
    setSelected(undefined);
  };

  const onUpdate = (entry: DictionaryEntry) => {
    if (isNaN(entry.boost)) {
      entry.boost = 0;
    }
    invoke("validate", { bopomofo: entry.bopomofo }).catch((e) => {
      message(e, { title: "錯誤", kind: "error" });
    });
    const nextItems = [...items];
    nextItems[selected!] = entry;
    setItems(nextItems);
    setItemsView(view(nextItems, filter));
  };

  const onSave = () => {
    invoke("save", { entries: items })
      .then((value) => {
        const saved = value as DictionaryEntry[];
        setItems(saved);
        setItemsView(view(saved, filter));
        setSelected(undefined);
        saveComplete();
      })
      .catch((e) => {
        message(e, { title: "錯誤", kind: "error" });
      });
  };

  const onSearch = (_e: any, data: InputOnChangeData) => {
    setFilter(data.value);
    setItemsView(view(items, data.value));
    setSelected(undefined);
  };

  useEffect(() => {
    const observer = new ResizeObserver((entries) => {
      entries.forEach((el) => {
        setGridHeight(el.contentRect.height - 20);
      });
    });
    if (ref.current) {
      observer.observe(ref.current);
    }
    return () => {
      observer.disconnect();
    };
  }, []);

  return (
    <div className={styles.root}>
      <div className={styles.leftPanel}>
        <div className={styles.topPanel}>
          <Button onClick={onInsert}>新增</Button>
          <Button onClick={onDelete} disabled={selected === undefined}>
            刪除
          </Button>
          <Button onClick={onSave}>存檔</Button>
        </div>
        <Input
          className={styles.search}
          placeholder="搜尋..."
          onChange={onSearch}
        />
        <div ref={ref} className={styles.bottomPanel}>
          <DataGrid
            className={styles.grid}
            items={itemsView}
            columns={columns}
            resizableColumns={true}
            sortable={true}
            defaultSortState={{
              sortColumn: "reading",
              sortDirection: "ascending",
            }}
            columnSizingOptions={{
              word: {
                defaultWidth: 60,
                autoFitColumns: true,
                minWidth: 60,
                idealWidth: 60,
              },
              bopomofo: {
                defaultWidth: 100,
                autoFitColumns: true,
                minWidth: 100,
                idealWidth: 150,
              },
              boost: {
                defaultWidth: 80,
                autoFitColumns: true,
                minWidth: 80,
                idealWidth: 80,
              },
            }}
            selectionMode="single"
            subtleSelection={true}
            // Row ids are positions in the view, which a delete or save
            // shifts, so the grid follows the entry selected here.
            selectedItems={
              selected === undefined
                ? []
                : [itemsView.findIndex((v) => v.index == selected)]
            }
            onSelectionChange={selectHandler}
          >
            <DataGridHeader>
              <DataGridRow>
                {({ renderHeaderCell }) => (
                  <DataGridHeaderCell>{renderHeaderCell()}</DataGridHeaderCell>
                )}
              </DataGridRow>
            </DataGridHeader>
            <DataGridBody<DictionaryEntry> itemSize={40} height={gridHeight}>
              {renderRow}
            </DataGridBody>
          </DataGrid>
        </div>
      </div>
      <div className={styles.rightPanel}>
        <WordEditor
          key={selected}
          disabled={selected === undefined}
          word={selected !== undefined ? items[selected].word : ""}
          bopomofo={selected !== undefined ? items[selected].bopomofo : ""}
          boost={selected !== undefined ? items[selected].boost: 0}
          onChange={onUpdate}
        />
      </div>
      <Toaster toasterId={toasterId} />
    </div>
  );
}

export default DictionaryEditor;
export type { DictionaryEntry };
