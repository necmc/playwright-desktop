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


FONT_PATH = Path(reportlab.__file__).parent / "fonts" / "Vera.ttf"
pdfmetrics.registerFont(TTFont("DemoSans", str(FONT_PATH)))


def create_report(data: dict[str, str]) -> bytes:
    output = BytesIO()
    document = SimpleDocTemplate(
        output, pagesize=A4, rightMargin=52, leftMargin=52,
        topMargin=56, bottomMargin=52, title="Información del equipo",
        author="QA Automation Interop",
    )
    title_style = ParagraphStyle("Title", fontName="DemoSans", fontSize=22, leading=28)
    text_style = ParagraphStyle("Value", fontName="DemoSans", fontSize=11, leading=17)
    muted_style = ParagraphStyle("Muted", parent=text_style, textColor=colors.HexColor("#64748b"))
    rows = [
        [Paragraph(label, text_style), Paragraph(escape(data[key]), text_style)]
        for label, key in (("Fecha", "date"), ("Nombre del equipo", "computerName"), ("Alias", "alias"))
    ]
    table = Table(rows, colWidths=[155, A4[0] - 259])
    table.setStyle(TableStyle([
        ("BACKGROUND", (0, 0), (0, -1), colors.HexColor("#f1f5f9")),
        ("LINEBELOW", (0, 0), (-1, -1), 0.5, colors.HexColor("#dce3ec")),
        ("VALIGN", (0, 0), (-1, -1), "TOP"),
        ("TOPPADDING", (0, 0), (-1, -1), 15),
        ("BOTTOMPADDING", (0, 0), (-1, -1), 15),
        ("LEFTPADDING", (0, 0), (-1, -1), 12),
        ("RIGHTPADDING", (0, 0), (-1, -1), 12),
    ]))
    document.build([
        Paragraph("QA Automation Interop", muted_style), Spacer(1, 22),
        Paragraph("Información del equipo", title_style), Spacer(1, 12),
        Paragraph("Datos confirmados en la aplicación de escritorio.", muted_style),
        Spacer(1, 28), table,
    ])
    return output.getvalue()
