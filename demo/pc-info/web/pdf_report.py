"""Informe de los valores confirmados en la aplicación Java."""

from io import BytesIO
from pathlib import Path
from xml.sax.saxutils import escape

import reportlab
from reportlab.lib import colors
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import ParagraphStyle
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.platypus import Paragraph, SimpleDocTemplate, Spacer, Table, TableStyle

FONT_NAME = "DemoSans"
FONT_FILENAME = "Vera.ttf"
DOCUMENT_TITLE = "Información del equipo"
DOCUMENT_AUTHOR = "playwright-desktop"
SUBTITLE_TEXT = "Datos confirmados en la aplicación de escritorio."

PAGE_RIGHT_MARGIN = 52
PAGE_LEFT_MARGIN = 52
PAGE_TOP_MARGIN = 56
PAGE_BOTTOM_MARGIN = 52

TITLE_FONT_SIZE = 22
TITLE_LEADING = 28
TEXT_FONT_SIZE = 11
TEXT_LEADING = 17

MUTED_COLOR = "#64748b"
ROW_ALT_BG = "#f1f5f9"
ROW_BORDER_COLOR = "#dce3ec"

LABEL_COLUMN_WIDTH = 155
VALUE_COLUMN_PADDING = 259
BORDER_WIDTH = 0.5
CELL_PADDING_Y = 15
CELL_PADDING_X = 12
SPACER_AFTER_BRAND = 22
SPACER_AFTER_TITLE = 12
SPACER_AFTER_SUBTITLE = 28

REPORT_FIELDS = (
    ("Fecha", "date"),
    ("Nombre del equipo", "computerName"),
    ("Alias", "alias"),
)

FONT_PATH = Path(reportlab.__file__).parent / "fonts" / FONT_FILENAME
pdfmetrics.registerFont(TTFont(FONT_NAME, str(FONT_PATH)))


def create_report(data: dict[str, str]) -> bytes:
    output = BytesIO()
    document = SimpleDocTemplate(
        output,
        pagesize=A4,
        rightMargin=PAGE_RIGHT_MARGIN,
        leftMargin=PAGE_LEFT_MARGIN,
        topMargin=PAGE_TOP_MARGIN,
        bottomMargin=PAGE_BOTTOM_MARGIN,
        title=DOCUMENT_TITLE,
        author=DOCUMENT_AUTHOR,
    )
    title_style = ParagraphStyle("Title", fontName=FONT_NAME, fontSize=TITLE_FONT_SIZE, leading=TITLE_LEADING)
    text_style = ParagraphStyle("Value", fontName=FONT_NAME, fontSize=TEXT_FONT_SIZE, leading=TEXT_LEADING)
    muted_style = ParagraphStyle("Muted", parent=text_style, textColor=colors.HexColor(MUTED_COLOR))
    rows = [
        [Paragraph(label, text_style), Paragraph(escape(data[key]), text_style)]
        for label, key in REPORT_FIELDS
    ]
    table = Table(rows, colWidths=[LABEL_COLUMN_WIDTH, A4[0] - VALUE_COLUMN_PADDING])
    table.setStyle(TableStyle([
        ("BACKGROUND", (0, 0), (0, -1), colors.HexColor(ROW_ALT_BG)),
        ("LINEBELOW", (0, 0), (-1, -1), BORDER_WIDTH, colors.HexColor(ROW_BORDER_COLOR)),
        ("VALIGN", (0, 0), (-1, -1), "TOP"),
        ("TOPPADDING", (0, 0), (-1, -1), CELL_PADDING_Y),
        ("BOTTOMPADDING", (0, 0), (-1, -1), CELL_PADDING_Y),
        ("LEFTPADDING", (0, 0), (-1, -1), CELL_PADDING_X),
        ("RIGHTPADDING", (0, 0), (-1, -1), CELL_PADDING_X),
    ]))
    document.build([
        Paragraph(DOCUMENT_AUTHOR, muted_style), Spacer(1, SPACER_AFTER_BRAND),
        Paragraph(DOCUMENT_TITLE, title_style), Spacer(1, SPACER_AFTER_TITLE),
        Paragraph(SUBTITLE_TEXT, muted_style),
        Spacer(1, SPACER_AFTER_SUBTITLE), table,
    ])
    return output.getvalue()
