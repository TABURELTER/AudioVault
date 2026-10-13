from pathlib import Path
p=Path('apps/desktop/src/App.tsx');s=p.read_text(encoding='utf-8');s=s.replace('  const selectionRequest = useRef(0);', '  const selectionRequest = useRef(0);\n  const activeInspector = useRef<{ book: number | null; ebook: number | null }>({ book: null, ebook: null });\n  const [catalogRevision, setCatalogRevision] = useState(0);')
s=s.replace('    let disposed = false;\n    api', '''    let disposed = false;
    const refreshInspector = () => {
      const { book, ebook } = activeInspector.current;
      if (book === null && ebook === null) return;
      const request = ++selectionRequest.current;
      if (book !== null) api.getBookDetails(book).then(detail => {
        if (request === selectionRequest.current && detail) setSelectedBook(detail);
      });
      else if (ebook !== null) api.getEbookDetail(ebook).then(detail => {
        if (request === selectionRequest.current && detail) setSelectedEbook(detail);
      });
    };
    api''',1)
s=s.replace('      .onEvent((event) => {\n        if', '      .onEvent((event) => {\n        if (disposed) return;\n        if',1)
s=s.replace('        if (event === "library_changed") {\n          reloadBooks();', '        if (event === "library_changed") {\n          setCatalogRevision(value => value + 1);\n          refreshInspector();\n          reloadBooks();')
s=s.replace('        } else if (event === "volumes_changed") {\n          reloadVolumes();', '        } else if (event === "volumes_changed") {\n          setCatalogRevision(value => value + 1);\n          refreshInspector();\n          reloadBooks();\n          reloadVolumes();')
s=s.replace('  const handleSelectBook = async (bookId: number) => {', '  const handleSelectBook = async (bookId: number) => {\n    activeInspector.current = { book: bookId, ebook: null };')
s=s.replace('  const handleSelectEbook = async (ebookId: number) => {', '  const handleSelectEbook = async (ebookId: number) => {\n    activeInspector.current = { book: null, ebook: ebookId };')
s=s.replace('  const handleCloseDrawer = () => {', '  const handleCloseDrawer = () => {\n    activeInspector.current = { book: null, ebook: null };')
s=s.replace('  const handleCloseEbookDrawer = () => {', '  const handleCloseEbookDrawer = () => {\n    activeInspector.current = { book: null, ebook: null };')
s=s.replace('        selectionRequest.current++;\n        requestAnimationFrame', '        selectionRequest.current++;\n        activeInspector.current = { book: null, ebook: null };\n        requestAnimationFrame')
s=s.replace('            <EbooksPage\n', '            <EbooksPage\n              catalogRevision={catalogRevision}\n')
p.write_text(s,encoding='utf-8')
p=Path('apps/desktop/src/pages/EbooksPage.tsx');s=p.read_text(encoding='utf-8');s=s.replace('interface Props {\n', 'interface Props {\n  catalogRevision?: number;\n',1).replace('export function EbooksPage({\n', 'export function EbooksPage({\n  catalogRevision = 0,\n',1).replace('[debounced, format, sort]', '[debounced, format, sort, catalogRevision]');p.write_text(s,encoding='utf-8')
